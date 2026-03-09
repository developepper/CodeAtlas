//! Pipeline stage implementations: discovery, parse, persist.
//!
//! Each stage is a free function that takes a [`PipelineContext`] (or the
//! output of the previous stage) and returns a typed result.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use adapter_api::{AdapterError, AdapterOutput, SourceFile};
use core_model::{
    build_symbol_id, current_index_schema_version, FileRecord, QualityLevel, QualityMix,
    RepoRecord, SymbolRecord, Validate,
};
use repo_walker::{detect_language, walk_repository, WalkerOptions};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::{debug, info, warn};

use crate::context::PipelineContext;
use crate::PipelineError;

// ---------------------------------------------------------------------------
// Discovery stage
// ---------------------------------------------------------------------------

/// Output of the discovery stage: files ready for parsing.
#[derive(Debug)]
pub struct DiscoveryOutput {
    /// Files discovered, with language detected and content loaded.
    pub files: Vec<PreparedFile>,
    /// Discovery metrics from the walker.
    pub metrics: repo_walker::DiscoveryMetrics,
}

/// A file ready to be sent through the parse stage.
#[derive(Debug)]
pub struct PreparedFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub language: String,
    pub content: Vec<u8>,
}

/// Runs discovery: walks the repository, detects languages, and loads file
/// content. Files with no detected language are skipped.
pub fn discover(ctx: &PipelineContext<'_>) -> Result<DiscoveryOutput, PipelineError> {
    let walker_opts = WalkerOptions {
        correlation_id: ctx.correlation_id.clone(),
        ..WalkerOptions::default()
    };

    let walk_result =
        walk_repository(ctx.source_root(), &walker_opts).map_err(PipelineError::Discovery)?;

    info!(
        files_discovered = walk_result.metrics.files_discovered,
        "discovery stage complete"
    );

    let mut prepared = Vec::with_capacity(walk_result.files.len());
    for file in &walk_result.files {
        // Read content first so language detection can use content-based
        // fallbacks (shebang, magic bytes) without depending on CWD.
        let content = fs::read(&file.absolute_path).map_err(|source| PipelineError::Io {
            path: Some(file.absolute_path.clone()),
            source,
        })?;

        let lang = detect_language(&file.relative_path, &content);
        if lang == repo_walker::Language::Unknown {
            debug!(
                path = %file.relative_path.display(),
                "skipping file with unknown language"
            );
            continue;
        }
        let lang = lang.as_str().to_string();

        prepared.push(PreparedFile {
            relative_path: file.relative_path.clone(),
            absolute_path: file.absolute_path.clone(),
            language: lang,
            content,
        });
    }

    Ok(DiscoveryOutput {
        files: prepared,
        metrics: walk_result.metrics,
    })
}

// ---------------------------------------------------------------------------
// Parse stage
// ---------------------------------------------------------------------------

/// Result of parsing a single file through adapters.
#[derive(Debug)]
pub struct ParsedFile {
    pub relative_path: PathBuf,
    pub language: String,
    pub output: AdapterOutput,
    pub content_hash: String,
    /// Raw file content, carried through for blob storage.
    pub content: Vec<u8>,
}

/// Per-file error captured during the parse stage (non-fatal).
///
/// Carries the failing adapter's identity when available, so callers can
/// trace failures back to a specific adapter.
#[derive(Debug)]
pub struct FileError {
    pub path: PathBuf,
    /// The adapter that produced the error, if applicable.
    pub adapter_id: Option<String>,
    pub error: String,
}

/// Output of the parse stage.
#[derive(Debug)]
pub struct ParseOutput {
    pub parsed_files: Vec<ParsedFile>,
    pub file_errors: Vec<FileError>,
}

/// Runs the parse stage: for each prepared file, selects adapters via the
/// router and invokes them. Uses the first adapter that succeeds.
///
/// Individual file failures are recorded in `file_errors`; the pipeline
/// continues with the remaining files.
pub fn parse(ctx: &PipelineContext<'_>, discovery: &DiscoveryOutput) -> ParseOutput {
    let idx_ctx = ctx.index_context();
    let mut parsed_files = Vec::new();
    let mut file_errors = Vec::new();

    for file in &discovery.files {
        let policy = ctx.default_policy;
        let adapters = ctx.router.select(&file.language, policy);

        if adapters.is_empty() {
            debug!(
                path = %file.relative_path.display(),
                language = %file.language,
                "no adapter available, skipping"
            );
            file_errors.push(FileError {
                path: file.relative_path.clone(),
                adapter_id: None,
                error: format!("no adapter for language '{}'", file.language),
            });
            continue;
        }

        let source_file = SourceFile {
            relative_path: file.relative_path.clone(),
            absolute_path: file.absolute_path.clone(),
            content: file.content.clone(),
            language: file.language.clone(),
        };

        // Try adapters in priority order; use the first that succeeds.
        // Non-fatal errors are recorded but do not prevent lower-priority
        // adapters from being tried.
        let mut succeeded = false;
        let mut per_file_errors: Vec<FileError> = Vec::new();
        for adapter in &adapters {
            match adapter.index_file(&idx_ctx, &source_file) {
                Ok(output) => {
                    let content_hash = file_content_hash(&file.content);
                    parsed_files.push(ParsedFile {
                        relative_path: file.relative_path.clone(),
                        language: file.language.clone(),
                        output,
                        content_hash,
                        content: file.content.clone(),
                    });
                    succeeded = true;
                    break;
                }
                Err(AdapterError::Unsupported { .. }) => continue,
                Err(e) => {
                    warn!(
                        path = %file.relative_path.display(),
                        adapter = adapter.adapter_id(),
                        error = %e,
                        "adapter failed, trying next"
                    );
                    per_file_errors.push(FileError {
                        path: file.relative_path.clone(),
                        adapter_id: Some(adapter.adapter_id().to_string()),
                        error: e.to_string(),
                    });
                    continue;
                }
            }
        }

        if !succeeded {
            if per_file_errors.is_empty() {
                file_errors.push(FileError {
                    path: file.relative_path.clone(),
                    adapter_id: None,
                    error: "all adapters returned unsupported".to_string(),
                });
            } else {
                file_errors.append(&mut per_file_errors);
            }
        }
    }

    info!(
        parsed = parsed_files.len(),
        errors = file_errors.len(),
        "parse stage complete"
    );

    ParseOutput {
        parsed_files,
        file_errors,
    }
}

/// Computes the SHA-256 content hash for a file's content.
///
/// Delegates to [`store::content_hash`] so the indexer and blob store
/// use the same canonical hash function.
fn file_content_hash(content: &[u8]) -> String {
    store::content_hash(content)
}

// ---------------------------------------------------------------------------
// Persist stage
// ---------------------------------------------------------------------------

/// Persists parsed results to the metadata and blob stores.
///
/// Content blobs are written first (idempotent, content-addressed) so they
/// are available before the metadata transaction opens. Metadata writes
/// (repo → files → symbols) happen inside a single SQLite transaction:
/// either everything commits or nothing does.
///
/// Stale data cleanup: files no longer present in the current **discovery**
/// are deleted (cascading to their symbols). The discovery output — not the
/// parse output — drives stale detection so that files which were discovered
/// but failed parsing (e.g. transient adapter errors) are preserved rather
/// than incorrectly purged. Symbols removed from a file since the last
/// index are cleaned up before upserting the new set.
///
/// Repo-level aggregates (`file_count`, `symbol_count`, `language_counts`)
/// are recomputed from actual database state after all upserts and deletes,
/// ensuring consistency across re-indexes.
///
/// Any validation failure aborts the entire transaction (automatic rollback
/// on drop) to prevent inconsistent state between aggregate counts and
/// actual persisted records.
pub fn persist(
    ctx: &PipelineContext<'_>,
    store: &mut store::MetadataStore,
    blob_store: &store::BlobStore,
    discovery: &DiscoveryOutput,
    parse_output: &ParseOutput,
) -> Result<(), PipelineError> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| PipelineError::Internal(format!("timestamp format error: {e}")))?;

    // -- Pass 1: collect per-file stats and validate symbol IDs upfront --

    struct FileStats {
        symbol_count: u64,
        semantic_count: u64,
    }

    let mut file_stats: Vec<FileStats> = Vec::with_capacity(parse_output.parsed_files.len());

    for parsed in &parse_output.parsed_files {
        let sym_count = parsed.output.symbols.len() as u64;

        // Validate all symbol IDs upfront — any failure is fatal.
        for sym in &parsed.output.symbols {
            let file_path_str = parsed.relative_path.to_string_lossy();
            build_symbol_id(&file_path_str, &sym.qualified_name, sym.kind).map_err(|e| {
                PipelineError::Validation(format!(
                    "invalid symbol ID for '{}' in {}: {e}",
                    sym.name, file_path_str
                ))
            })?;
        }

        let semantic = if parsed.output.quality_level == QualityLevel::Semantic {
            sym_count
        } else {
            0
        };

        file_stats.push(FileStats {
            symbol_count: sym_count,
            semantic_count: semantic,
        });
    }

    // -- Validate a provisional repo record before opening transaction --

    let schema_version = current_index_schema_version();
    let provisional_repo = RepoRecord {
        repo_id: ctx.repo_id.clone(),
        display_name: ctx.repo_id.clone(),
        source_root: ctx.source_root.to_string_lossy().to_string(),
        indexed_at: now.clone(),
        index_version: schema_version.to_string(),
        // Placeholder aggregates — will be recomputed from DB after writes.
        language_counts: BTreeMap::new(),
        file_count: 0,
        symbol_count: 0,
        git_head: None,
    };

    if let Err(e) = provisional_repo.validate() {
        return Err(PipelineError::Validation(format!(
            "repo record validation failed: {e}"
        )));
    }

    // -- Write content blobs (idempotent, before metadata transaction) --

    for parsed in &parse_output.parsed_files {
        blob_store.put(&parsed.content).map_err(|e| {
            PipelineError::Persist(store::StoreError::Blob {
                path: Some(parsed.relative_path.clone()),
                reason: format!("failed to write blob: {e}"),
            })
        })?;
    }

    // -- Begin atomic metadata transaction --

    let tx = store.transaction().map_err(PipelineError::Persist)?;

    // Step 1: ensure repo record exists without cascade-deleting children.
    // Uses INSERT OR IGNORE + UPDATE to avoid ON DELETE CASCADE that
    // INSERT OR REPLACE would trigger on an existing repo.
    tx.repos()
        .ensure_and_update(&provisional_repo)
        .map_err(PipelineError::Persist)?;

    // Step 2: remove stale files (cascade deletes their symbols).
    // Use the discovery output (all files found on disk) rather than parse
    // output (only successfully parsed files) so that adapter failures do
    // not cause previously indexed metadata to be incorrectly purged.
    let current_paths: Vec<&str> = discovery
        .files
        .iter()
        .map(|f| {
            f.relative_path.to_str().ok_or_else(|| {
                PipelineError::Internal(format!(
                    "non-UTF8 file path: {}",
                    f.relative_path.display()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let stale_deleted = tx
        .files()
        .delete_except(&ctx.repo_id, &current_paths)
        .map_err(PipelineError::Persist)?;
    if stale_deleted > 0 {
        info!(
            stale_files_removed = stale_deleted,
            "cleaned up stale file records"
        );
    }

    // Step 3: upsert file records and their symbols
    for (parsed, stats) in parse_output.parsed_files.iter().zip(file_stats.iter()) {
        let file_path_str = parsed.relative_path.to_string_lossy();

        let quality_mix = if stats.symbol_count > 0 {
            let sem_pct = (stats.semantic_count as f32 / stats.symbol_count as f32) * 100.0;
            QualityMix {
                semantic_percent: sem_pct,
                syntax_percent: 100.0 - sem_pct,
            }
        } else {
            QualityMix {
                semantic_percent: 0.0,
                syntax_percent: 0.0,
            }
        };

        let file_record = FileRecord {
            repo_id: ctx.repo_id.clone(),
            file_path: file_path_str.to_string(),
            language: parsed.language.clone(),
            file_hash: parsed.content_hash.clone(),
            summary: format!("{} source file", parsed.language),
            symbol_count: stats.symbol_count,
            quality_mix,
            updated_at: now.clone(),
        };

        file_record.validate().map_err(|e| {
            PipelineError::Validation(format!(
                "file record validation failed for '{}': {e}",
                file_path_str
            ))
        })?;

        tx.files()
            .upsert(&file_record)
            .map_err(PipelineError::Persist)?;

        // Remove stale symbols for this file before upserting new ones.
        // This handles symbols that were removed or renamed since the
        // last index without relying on ID stability.
        tx.symbols()
            .delete_for_file(&ctx.repo_id, &file_path_str)
            .map_err(PipelineError::Persist)?;

        let default_confidence = match parsed.output.quality_level {
            QualityLevel::Semantic => 0.9,
            QualityLevel::Syntax => 0.7,
        };

        for sym in &parsed.output.symbols {
            // Symbol ID was pre-validated in pass 1.
            let symbol_id = build_symbol_id(&file_path_str, &sym.qualified_name, sym.kind)
                .map_err(|e| {
                    PipelineError::Validation(format!(
                        "invalid symbol ID for '{}' in {}: {e}",
                        sym.name, file_path_str
                    ))
                })?;

            let confidence = sym.confidence_score.unwrap_or(default_confidence);

            let record = SymbolRecord {
                id: symbol_id,
                repo_id: ctx.repo_id.clone(),
                file_path: file_path_str.to_string(),
                language: parsed.language.clone(),
                kind: sym.kind,
                name: sym.name.clone(),
                qualified_name: sym.qualified_name.clone(),
                signature: sym.signature.clone(),
                start_line: sym.span.start_line,
                end_line: sym.span.end_line,
                start_byte: sym.span.start_byte,
                byte_length: sym.span.byte_length,
                content_hash: parsed.content_hash.clone(),
                quality_level: parsed.output.quality_level,
                confidence_score: confidence,
                source_adapter: parsed.output.source_adapter.clone(),
                indexed_at: now.clone(),
                docstring: sym.docstring.clone(),
                summary: None,
                parent_symbol_id: None,
                keywords: None,
                decorators_or_attributes: None,
                semantic_refs: None,
            };

            record.validate().map_err(|e| {
                PipelineError::Validation(format!(
                    "symbol record validation failed for '{}' in {}: {e}",
                    sym.name, file_path_str
                ))
            })?;

            tx.symbols()
                .upsert(&record)
                .map_err(PipelineError::Persist)?;
        }
    }

    // Step 4: recompute repo aggregates from actual DB state.
    // Uses a targeted UPDATE (not INSERT OR REPLACE) to avoid triggering
    // ON DELETE CASCADE which would wipe the file/symbol records we just wrote.
    let file_count = tx
        .files()
        .count(&ctx.repo_id)
        .map_err(PipelineError::Persist)?;
    let symbol_count = tx
        .symbols()
        .count_for_repo(&ctx.repo_id)
        .map_err(PipelineError::Persist)?;
    let language_counts = tx
        .files()
        .aggregate_language_counts(&ctx.repo_id)
        .map_err(PipelineError::Persist)?;

    tx.repos()
        .update_aggregates(&ctx.repo_id, file_count, symbol_count, &language_counts)
        .map_err(PipelineError::Persist)?;

    // -- Commit the transaction atomically --
    tx.commit().map_err(PipelineError::Persist)?;

    info!(
        files_persisted = file_count,
        symbols_persisted = symbol_count,
        "persist stage complete"
    );

    Ok(())
}

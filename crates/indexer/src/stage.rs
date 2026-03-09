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
}

/// Per-file error captured during the parse stage (non-fatal).
#[derive(Debug)]
pub struct FileError {
    pub path: PathBuf,
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
        let mut succeeded = false;
        for adapter in &adapters {
            match adapter.index_file(&idx_ctx, &source_file) {
                Ok(output) => {
                    let content_hash = simple_content_hash(&file.content);
                    parsed_files.push(ParsedFile {
                        relative_path: file.relative_path.clone(),
                        language: file.language.clone(),
                        output,
                        content_hash,
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
                        "adapter failed"
                    );
                    file_errors.push(FileError {
                        path: file.relative_path.clone(),
                        error: e.to_string(),
                    });
                    break;
                }
            }
        }

        if !succeeded
            && file_errors
                .last()
                .map_or(true, |e| e.path != file.relative_path)
        {
            file_errors.push(FileError {
                path: file.relative_path.clone(),
                error: "all adapters returned unsupported".to_string(),
            });
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

/// Simple deterministic content hash (sum of bytes mod u64). Production
/// will use SHA-256 once blob storage (#29) lands.
fn simple_content_hash(content: &[u8]) -> String {
    let mut hash: u64 = 0;
    for &byte in content {
        hash = hash.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Persist stage
// ---------------------------------------------------------------------------

/// Persists parsed results to the metadata store.
///
/// Respects FK ordering: repo → files → symbols. Validation failures on
/// individual records are logged and skipped (non-fatal).
pub fn persist(ctx: &PipelineContext<'_>, parse_output: &ParseOutput) -> Result<(), PipelineError> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| PipelineError::Internal(format!("timestamp format error: {e}")))?;

    // -- Pass 1: collect per-file stats for repo + file records --

    let mut language_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_symbol_count: u64 = 0;

    struct FileStats {
        symbol_count: u64,
        semantic_count: u64,
    }

    let mut file_stats: Vec<FileStats> = Vec::with_capacity(parse_output.parsed_files.len());

    for parsed in &parse_output.parsed_files {
        let valid_symbols = parsed
            .output
            .symbols
            .iter()
            .filter(|sym| {
                let file_path_str = parsed.relative_path.to_string_lossy();
                build_symbol_id(&file_path_str, &sym.qualified_name, sym.kind).is_ok()
            })
            .count() as u64;

        let semantic = if parsed.output.quality_level == QualityLevel::Semantic {
            valid_symbols
        } else {
            0
        };

        file_stats.push(FileStats {
            symbol_count: valid_symbols,
            semantic_count: semantic,
        });

        *language_counts.entry(parsed.language.clone()).or_insert(0) += 1;
        total_symbol_count += valid_symbols;
    }

    // -- Step 1: upsert repo record (no FK parent) --

    let schema_version = current_index_schema_version();
    let repo_record = RepoRecord {
        repo_id: ctx.repo_id.clone(),
        display_name: ctx.repo_id.clone(),
        source_root: ctx.source_root.to_string_lossy().to_string(),
        indexed_at: now.clone(),
        index_version: schema_version.to_string(),
        language_counts,
        file_count: parse_output.parsed_files.len() as u64,
        symbol_count: total_symbol_count,
        git_head: None,
    };

    if let Err(e) = repo_record.validate() {
        return Err(PipelineError::Validation(format!(
            "repo record validation failed: {e}"
        )));
    }

    ctx.store
        .repos()
        .upsert(&repo_record)
        .map_err(PipelineError::Persist)?;

    // -- Step 2: upsert file records (FK → repos) --

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

        if let Err(e) = file_record.validate() {
            warn!(
                path = %file_path_str,
                error = %e,
                "skipping file record that failed validation"
            );
            continue;
        }

        ctx.store
            .files()
            .upsert(&file_record)
            .map_err(PipelineError::Persist)?;
    }

    // -- Step 3: upsert symbol records (FK → files) --

    for parsed in &parse_output.parsed_files {
        let file_path_str = parsed.relative_path.to_string_lossy();
        let default_confidence = match parsed.output.quality_level {
            QualityLevel::Semantic => 0.9,
            QualityLevel::Syntax => 0.7,
        };

        for sym in &parsed.output.symbols {
            let symbol_id = match build_symbol_id(&file_path_str, &sym.qualified_name, sym.kind) {
                Ok(id) => id,
                Err(e) => {
                    warn!(
                        path = %file_path_str,
                        symbol = %sym.name,
                        error = %e,
                        "skipping symbol with invalid ID"
                    );
                    continue;
                }
            };

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

            if let Err(e) = record.validate() {
                warn!(
                    symbol_id = %record.id,
                    error = %e,
                    "skipping symbol that failed validation"
                );
                continue;
            }

            ctx.store
                .symbols()
                .upsert(&record)
                .map_err(PipelineError::Persist)?;
        }
    }

    info!(
        files_persisted = parse_output.parsed_files.len(),
        symbols_persisted = total_symbol_count,
        "persist stage complete"
    );

    Ok(())
}

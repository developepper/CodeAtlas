//! Pipeline stage implementations: discovery, extract, persist.
//!
//! Each stage is a free function that takes a [`PipelineContext`] (or the
//! output of the previous stage) and returns a typed result.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use core_model::{
    build_symbol_id, current_index_schema_version, FileRecord, FreshnessStatus, IndexingStatus,
    RepoRecord, SymbolRecord, Validate,
};
use repo_walker::{detect_language, walk_repository, WalkerOptions};
use syntax_platform::PreparedFile;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::{debug, info, info_span, warn};

use crate::classify::{CapabilityClassifier, DefaultCapabilityClassifier};
use crate::context::PipelineContext;
use crate::dispatch::{DefaultDispatchPlanner, DispatchPlanner, ExecutionPlan};
use crate::enrich;
use crate::merge_engine::{
    BackendAttempt, DefaultMergeEngine, ExecutionOutcome, MergeEngine, MergeResult,
    MergedSymbolProvenance,
};
use crate::PipelineError;

// ---------------------------------------------------------------------------
// Re-export PreparedFile from syntax-platform
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Discovery stage
// ---------------------------------------------------------------------------

/// Output of the discovery stage: files ready for extraction.
#[derive(Debug)]
pub struct DiscoveryOutput {
    /// Files discovered, with language detected and content loaded.
    pub files: Vec<PreparedFile>,
    /// Discovery metrics from the walker.
    pub metrics: repo_walker::DiscoveryMetrics,
}

/// Runs discovery: walks the repository, detects languages, and loads file
/// content. Files with no detected language are skipped.
pub fn discover(ctx: &PipelineContext<'_>) -> Result<DiscoveryOutput, PipelineError> {
    let span = info_span!("stage_discover", repo_id = %ctx.repo_id);
    let _guard = span.enter();

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
// Extract stage (replaces old parse stage)
// ---------------------------------------------------------------------------

/// Result of extracting symbols from a single file.
#[derive(Debug)]
pub struct ParsedFile {
    pub relative_path: PathBuf,
    pub language: String,
    pub merge_result: MergeResult,
    /// Per-symbol provenance, parallel to `merge_result.symbols`.
    pub symbol_provenance: Vec<MergedSymbolProvenance>,
    pub content_hash: String,
    /// Raw file content, carried through for blob storage.
    pub content: Vec<u8>,
}

/// Per-file error captured during the extract stage (non-fatal).
#[derive(Debug)]
pub struct FileError {
    pub path: PathBuf,
    /// The backend that produced the error, if applicable.
    pub backend_id: Option<String>,
    pub error: String,
}

/// Output of the extract stage.
#[derive(Debug)]
pub struct ParseOutput {
    pub parsed_files: Vec<ParsedFile>,
    pub file_errors: Vec<FileError>,
}

/// Runs the extract stage: for each prepared file, dispatches to syntax
/// and semantic backends, merges results, and classifies capability tier.
pub fn parse(ctx: &PipelineContext<'_>, discovery: &DiscoveryOutput) -> ParseOutput {
    let span = info_span!(
        "stage_extract",
        repo_id = %ctx.repo_id,
        files_to_extract = discovery.files.len(),
    );
    let _guard = span.enter();

    let planner = DefaultDispatchPlanner;
    let merge_engine = DefaultMergeEngine;
    let classifier = DefaultCapabilityClassifier;

    let mut parsed_files = Vec::new();
    let mut file_errors = Vec::new();

    for file in &discovery.files {
        let plan = planner.plan(file, ctx.registry, &ctx.dispatch_context);

        match plan {
            ExecutionPlan::FileOnly { ref reason } => {
                debug!(
                    path = %file.relative_path.display(),
                    language = %file.language,
                    reason = ?reason,
                    "file-only indexing"
                );

                let outcome = ExecutionOutcome {
                    plan: plan.clone(),
                    syntax_attempts: vec![],
                    semantic_attempts: vec![],
                    merge_result: None,
                };
                let capability_tier = classifier.classify(&outcome);

                let content_hash = file_content_hash(&file.content);
                let merge_result = MergeResult {
                    symbols: vec![],
                    provenance: vec![],
                    capability_tier,
                    duplicates_resolved: 0,
                };

                info!(
                    file_path = %file.relative_path.display(),
                    language = %file.language,
                    plan_type = "file_only",
                    file_only_reason = ?reason,
                    final_capability_tier = %capability_tier,
                    "file processing complete"
                );

                parsed_files.push(ParsedFile {
                    relative_path: file.relative_path.clone(),
                    language: file.language.clone(),
                    symbol_provenance: vec![],
                    merge_result,
                    content_hash,
                    content: file.content.clone(),
                });
            }
            ExecutionPlan::Execute {
                ref syntax,
                ref semantic,
            } => {
                // Run syntax backends.
                let mut syntax_attempts = Vec::new();
                for backend_id in syntax {
                    let backend = ctx.registry.syntax(backend_id);
                    let result = backend.extract_symbols(file);
                    if let Err(ref e) = result {
                        warn!(
                            path = %file.relative_path.display(),
                            backend = %backend_id,
                            error = %e,
                            "syntax backend failed"
                        );
                    }
                    syntax_attempts.push(BackendAttempt {
                        backend: backend_id.clone(),
                        result,
                    });
                }

                // Collect successful syntax extractions and merge into baseline.
                let successful_syntax: Vec<_> = syntax_attempts
                    .iter()
                    .filter_map(|a| a.result.as_ref().ok())
                    .cloned()
                    .collect();
                let syntax_baseline = merge_engine.merge_syntax(&successful_syntax);

                // Run semantic backends.
                let mut semantic_attempts = Vec::new();
                for backend_id in semantic {
                    let backend = ctx.registry.semantic(backend_id);
                    let result = backend.enrich_symbols(file, syntax_baseline.as_ref());
                    if let Err(ref e) = result {
                        warn!(
                            path = %file.relative_path.display(),
                            backend = %backend_id,
                            error = %e,
                            "semantic backend failed"
                        );
                    }
                    semantic_attempts.push(BackendAttempt {
                        backend: backend_id.clone(),
                        result,
                    });
                }

                // Collect successful semantic extractions.
                let successful_semantic: Vec<_> = semantic_attempts
                    .iter()
                    .filter_map(|a| a.result.as_ref().ok())
                    .cloned()
                    .collect();

                // Final merge.
                let merge_result =
                    merge_engine.merge_final(syntax_baseline.as_ref(), &successful_semantic);

                // Build the full execution outcome for classification.
                let outcome = ExecutionOutcome {
                    plan: plan.clone(),
                    syntax_attempts,
                    semantic_attempts,
                    merge_result: Some(merge_result.clone()),
                };
                let capability_tier = classifier.classify(&outcome);

                // Override the merge result's tier with the classifier's
                // authoritative determination (handles the
                // AllSyntaxBackendsFailed → FileOnly case).
                let merge_result = MergeResult {
                    capability_tier,
                    ..merge_result
                };

                // Emit structured diagnostic log per the architecture doc.
                let syntax_outcomes: Vec<&str> = outcome
                    .syntax_attempts
                    .iter()
                    .map(|a| if a.result.is_ok() { "success" } else { "error" })
                    .collect();
                let semantic_outcomes: Vec<&str> = outcome
                    .semantic_attempts
                    .iter()
                    .map(|a| if a.result.is_ok() { "success" } else { "error" })
                    .collect();

                info!(
                    file_path = %file.relative_path.display(),
                    language = %file.language,
                    plan_type = "execute",
                    planned_syntax_backends = ?syntax.iter().map(|b| &b.0).collect::<Vec<_>>(),
                    planned_semantic_backends = ?semantic.iter().map(|b| &b.0).collect::<Vec<_>>(),
                    syntax_outcomes = ?syntax_outcomes,
                    semantic_outcomes = ?semantic_outcomes,
                    final_capability_tier = %capability_tier,
                    "file processing complete"
                );

                // Record backend failures only if no usable output was produced.
                if merge_result.symbols.is_empty() {
                    for attempt in &outcome.syntax_attempts {
                        if let Err(e) = &attempt.result {
                            file_errors.push(FileError {
                                path: file.relative_path.clone(),
                                backend_id: Some(attempt.backend.0.clone()),
                                error: e.to_string(),
                            });
                        }
                    }
                    for attempt in &outcome.semantic_attempts {
                        if let Err(e) = &attempt.result {
                            file_errors.push(FileError {
                                path: file.relative_path.clone(),
                                backend_id: Some(attempt.backend.0.clone()),
                                error: e.to_string(),
                            });
                        }
                    }
                }

                let content_hash = file_content_hash(&file.content);
                parsed_files.push(ParsedFile {
                    relative_path: file.relative_path.clone(),
                    language: file.language.clone(),
                    symbol_provenance: merge_result.provenance.clone(),
                    merge_result,
                    content_hash,
                    content: file.content.clone(),
                });
            }
        }
    }

    info!(
        parsed = parsed_files.len(),
        errors = file_errors.len(),
        "extract stage complete"
    );

    ParseOutput {
        parsed_files,
        file_errors,
    }
}

/// Computes the SHA-256 content hash for a file's content.
fn file_content_hash(content: &[u8]) -> String {
    store::content_hash(content)
}

// ---------------------------------------------------------------------------
// Persist stage
// ---------------------------------------------------------------------------

/// Persists parsed results to the metadata and blob stores.
pub fn persist(
    ctx: &PipelineContext<'_>,
    store: &mut store::MetadataStore,
    blob_store: &store::BlobStore,
    discovery: &DiscoveryOutput,
    parse_output: &ParseOutput,
) -> Result<(), PipelineError> {
    let span = info_span!(
        "stage_persist",
        repo_id = %ctx.repo_id,
        files_to_persist = parse_output.parsed_files.len(),
    );
    let _guard = span.enter();

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| PipelineError::Internal(format!("timestamp format error: {e}")))?;

    // -- Pass 1: validate symbol IDs upfront --

    for parsed in &parse_output.parsed_files {
        for sym in &parsed.merge_result.symbols {
            let file_path_str = parsed.relative_path.to_string_lossy();
            build_symbol_id(&ctx.repo_id, &file_path_str, &sym.qualified_name, sym.kind).map_err(
                |e| {
                    PipelineError::Validation(format!(
                        "invalid symbol ID for '{}' in {}: {e}",
                        sym.name, file_path_str
                    ))
                },
            )?;
        }
    }

    // -- Validate a provisional repo record before opening transaction --

    let schema_version = current_index_schema_version();
    let provisional_repo = RepoRecord {
        repo_id: ctx.repo_id.clone(),
        display_name: ctx.repo_id.clone(),
        source_root: ctx.source_root.to_string_lossy().to_string(),
        indexed_at: now.clone(),
        index_version: schema_version.to_string(),
        language_counts: BTreeMap::new(),
        file_count: 0,
        symbol_count: 0,
        git_head: if ctx.use_git_diff {
            crate::git::current_head(ctx.source_root())
        } else {
            None
        },
        registered_at: Some(now.clone()),
        indexing_status: IndexingStatus::Ready,
        freshness_status: FreshnessStatus::Fresh,
    };

    if let Err(e) = provisional_repo.validate() {
        return Err(PipelineError::Validation(format!(
            "repo record validation failed: {e}"
        )));
    }

    // -- Write content blobs --

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

    tx.repos()
        .ensure_and_update(&provisional_repo)
        .map_err(PipelineError::Persist)?;

    // Remove stale files.
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

    // Upsert file records and their symbols.
    for parsed in &parse_output.parsed_files {
        let file_path_str = parsed.relative_path.to_string_lossy();
        let sym_count = parsed.merge_result.symbols.len() as u64;

        let file_record = FileRecord {
            repo_id: ctx.repo_id.clone(),
            file_path: file_path_str.to_string(),
            language: parsed.language.clone(),
            file_hash: parsed.content_hash.clone(),
            summary: enrich::file_summary(&parsed.language, &parsed.merge_result.symbols),
            symbol_count: sym_count,
            capability_tier: parsed.merge_result.capability_tier,
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

        // Remove stale symbols for this file.
        tx.symbols()
            .delete_for_file(&ctx.repo_id, &file_path_str)
            .map_err(PipelineError::Persist)?;

        for (sym_idx, sym) in parsed.merge_result.symbols.iter().enumerate() {
            let provenance = &parsed.symbol_provenance[sym_idx];

            let symbol_id =
                build_symbol_id(&ctx.repo_id, &file_path_str, &sym.qualified_name, sym.kind)
                    .map_err(|e| {
                        PipelineError::Validation(format!(
                            "invalid symbol ID for '{}' in {}: {e}",
                            sym.name, file_path_str
                        ))
                    })?;

            let keywords = enrich::extract_keywords(sym);
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
                capability_tier: provenance.capability_tier,
                confidence_score: provenance.confidence_score,
                source_backend: provenance.backend_id.0.clone(),
                indexed_at: now.clone(),
                docstring: sym.docstring.clone(),
                summary: Some(enrich::symbol_summary(sym)),
                parent_symbol_id: None,
                keywords: if keywords.is_empty() {
                    None
                } else {
                    Some(keywords)
                },
                decorators_or_attributes: None,
                semantic_refs: None,
                container_symbol_id: None,
                namespace_path: None,
                raw_kind: None,
                modifiers: None,
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

    // Recompute repo aggregates.
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

    tx.commit().map_err(PipelineError::Persist)?;

    info!(
        files_persisted = file_count,
        symbols_persisted = symbol_count,
        "persist stage complete"
    );

    Ok(())
}

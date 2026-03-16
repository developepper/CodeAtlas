//! Top-level pipeline orchestration.

use std::collections::HashSet;

use tracing::{info, info_span};

use crate::change_detection;
use crate::context::PipelineContext;
use crate::metrics::{self, CapabilityTierMetrics};
use crate::stage::{self, DiscoveryOutput, FileError, ParseOutput};
use crate::PipelineError;

/// Metrics collected across the full pipeline run.
#[derive(Debug)]
pub struct IndexMetrics {
    pub files_discovered: usize,
    pub files_parsed: usize,
    pub files_errored: usize,
    pub symbols_extracted: usize,
    /// Files skipped because their content hash matched the previous index.
    pub files_unchanged: usize,
    /// Files removed from the index because they were deleted from disk.
    pub files_deleted: usize,
    /// Files indexed at file level only (no symbols extracted).
    pub files_file_only: usize,
    /// Capability tier metrics computed from parse output.
    pub coverage: CapabilityTierMetrics,
}

/// Result of a successful pipeline run.
#[derive(Debug)]
pub struct IndexResult {
    pub metrics: IndexMetrics,
    pub file_errors: Vec<FileError>,
}

/// Runs the full indexing pipeline: discovery → extract → persist.
pub fn run(
    ctx: &PipelineContext<'_>,
    store: &mut store::MetadataStore,
    blob_store: &store::BlobStore,
) -> Result<IndexResult, PipelineError> {
    let correlation_id = ctx.correlation_id.as_deref().unwrap_or("");
    let span = info_span!(
        "index_pipeline",
        repo_id = %ctx.repo_id,
        correlation_id = %correlation_id,
    );
    let _guard = span.enter();

    info!("pipeline started");

    // Stage 1: Discovery
    let discovery: DiscoveryOutput = stage::discover(ctx)?;

    // Stage 1.5: Change detection
    let previous_hashes = store
        .files()
        .list_hash_map(&ctx.repo_id)
        .map_err(PipelineError::Persist)?;

    let change_set = if ctx.use_git_diff {
        let previous_head = store
            .repos()
            .get(&ctx.repo_id)
            .map_err(PipelineError::Persist)?
            .and_then(|r| r.git_head);

        match (previous_head, crate::git::current_head(ctx.source_root())) {
            (Some(prev), Some(curr)) => {
                info!(
                    previous_head = %prev,
                    current_head = %curr,
                    "attempting git-diff accelerated change detection"
                );
                change_detection::detect_changes_git(
                    &discovery.files,
                    &previous_hashes,
                    ctx.source_root(),
                    &prev,
                    &curr,
                )
                .unwrap_or_else(|| {
                    info!("git-diff failed, falling back to hash-based detection");
                    change_detection::detect_changes(&discovery.files, &previous_hashes)
                })
            }
            _ => {
                info!("git-diff not available, using hash-based detection");
                change_detection::detect_changes(&discovery.files, &previous_hashes)
            }
        }
    } else {
        change_detection::detect_changes(&discovery.files, &previous_hashes)
    };

    let files_unchanged = change_set.unchanged_count;
    let files_deleted = change_set.deleted_paths.len();

    if files_unchanged > 0 || files_deleted > 0 {
        info!(
            unchanged = files_unchanged,
            new = change_set.new_count,
            modified = change_set.modified_count,
            deleted = files_deleted,
            "change detection complete"
        );
    }

    // Build a filtered discovery output containing only changed/new files.
    let changed_discovery = DiscoveryOutput {
        files: change_set
            .changed_indices
            .iter()
            .map(|&i| {
                let f = &discovery.files[i];
                syntax_platform::PreparedFile {
                    relative_path: f.relative_path.clone(),
                    absolute_path: f.absolute_path.clone(),
                    language: f.language.clone(),
                    content: f.content.clone(),
                }
            })
            .collect(),
        metrics: discovery.metrics.clone(),
    };

    // Stage 2: Extract (dispatch → syntax → merge → semantic → final merge)
    let parse_output: ParseOutput = stage::parse(ctx, &changed_discovery);

    let symbols_extracted: usize = parse_output
        .parsed_files
        .iter()
        .map(|f| f.merge_result.symbols.len())
        .sum();

    // Compute capability tier metrics from parse output.
    let coverage = metrics::compute_tier_metrics(&parse_output);

    // Stage 3: Persist
    stage::persist(ctx, store, blob_store, &discovery, &parse_output)?;

    let files_file_only = coverage.files_file_only;
    let files_parsed = parse_output.parsed_files.len() - files_file_only;

    let metrics = IndexMetrics {
        files_discovered: discovery.metrics.files_discovered,
        files_parsed,
        files_errored: parse_output
            .file_errors
            .iter()
            .map(|e| &e.path)
            .collect::<HashSet<_>>()
            .len(),
        symbols_extracted,
        files_unchanged,
        files_deleted,
        files_file_only,
        coverage,
    };

    info!(
        files_discovered = metrics.files_discovered,
        files_parsed = metrics.files_parsed,
        files_file_only = metrics.files_file_only,
        files_errored = metrics.files_errored,
        files_unchanged = metrics.files_unchanged,
        files_deleted = metrics.files_deleted,
        symbols_extracted = metrics.symbols_extracted,
        semantic_symbols = metrics.coverage.semantic_symbols,
        syntax_symbols = metrics.coverage.syntax_symbols,
        semantic_coverage_percent = metrics.coverage.semantic_coverage_percent,
        avg_confidence = metrics.coverage.avg_confidence,
        duplicates_resolved = metrics.coverage.duplicates_resolved,
        files_with_semantic = metrics.coverage.files_with_semantic,
        win_rate = metrics.coverage.win_rate,
        wins = metrics.coverage.wins,
        losses = metrics.coverage.losses,
        ties = metrics.coverage.ties,
        "pipeline complete"
    );

    Ok(IndexResult {
        metrics,
        file_errors: parse_output.file_errors,
    })
}

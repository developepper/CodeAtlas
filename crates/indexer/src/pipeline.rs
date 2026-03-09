//! Top-level pipeline orchestration.

use tracing::info;

use crate::context::PipelineContext;
use crate::stage::{self, DiscoveryOutput, FileError, ParseOutput};
use crate::PipelineError;

/// Metrics collected across the full pipeline run.
#[derive(Debug)]
pub struct IndexMetrics {
    pub files_discovered: usize,
    pub files_parsed: usize,
    pub files_errored: usize,
    pub symbols_extracted: usize,
}

/// Result of a successful pipeline run.
#[derive(Debug)]
pub struct IndexResult {
    pub metrics: IndexMetrics,
    pub file_errors: Vec<FileError>,
}

/// Runs the full indexing pipeline: discovery → parse → persist.
///
/// The metadata store is passed separately from the pipeline context so that
/// only the persist stage borrows it mutably, while discovery and parse
/// operate with an immutable context.
///
/// Returns an [`IndexResult`] with aggregate metrics and any per-file
/// errors encountered during parsing.
pub fn run(
    ctx: &PipelineContext<'_>,
    store: &mut store::MetadataStore,
) -> Result<IndexResult, PipelineError> {
    info!(repo_id = %ctx.repo_id, "pipeline started");

    // Stage 1: Discovery
    let discovery: DiscoveryOutput = stage::discover(ctx)?;

    // Stage 2: Parse
    let parse_output: ParseOutput = stage::parse(ctx, &discovery);

    let symbols_extracted: usize = parse_output
        .parsed_files
        .iter()
        .map(|f| f.output.symbols.len())
        .sum();

    // Stage 3: Persist
    stage::persist(ctx, store, &parse_output)?;

    let metrics = IndexMetrics {
        files_discovered: discovery.metrics.files_discovered,
        files_parsed: parse_output.parsed_files.len(),
        files_errored: parse_output.file_errors.len(),
        symbols_extracted,
    };

    info!(
        files_discovered = metrics.files_discovered,
        files_parsed = metrics.files_parsed,
        files_errored = metrics.files_errored,
        symbols_extracted = metrics.symbols_extracted,
        "pipeline complete"
    );

    Ok(IndexResult {
        metrics,
        file_errors: parse_output.file_errors,
    })
}

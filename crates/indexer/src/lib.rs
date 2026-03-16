//! Indexer crate: pipeline orchestration for CodeAtlas.
//!
//! Orchestrates the indexing pipeline stages:
//! 1. **Discovery** — walk the repository and detect languages
//! 2. **Extract** — dispatch to syntax/semantic backends, merge results
//! 3. **Persist** — validate and write records to the metadata store

use std::path::PathBuf;

pub mod change_detection;
pub mod classify;
pub mod context;
pub mod dispatch;
pub mod enrich;
pub mod git;
pub mod merge_engine;
pub mod metrics;
pub mod pipeline;
pub mod registry;
pub mod stage;

pub use change_detection::{detect_changes, ChangeSet};
pub use classify::{CapabilityClassifier, DefaultCapabilityClassifier};
pub use context::PipelineContext;
pub use dispatch::{
    DefaultDispatchPlanner, DispatchContext, DispatchPlanner, ExecutionPlan, SemanticPolicy,
    SyntaxPolicy,
};
pub use merge_engine::{
    BackendAttempt, DefaultMergeEngine, ExecutionOutcome, MergeEngine, MergeResult, MergedSymbol,
    MergedSymbolProvenance,
};
pub use metrics::{compute_tier_metrics, CapabilityTierMetrics};
pub use pipeline::{run, IndexMetrics, IndexResult};
pub use registry::{BackendRegistry, DefaultBackendRegistry};
pub use stage::{DiscoveryOutput, FileError, ParseOutput, ParsedFile};

// Re-export PreparedFile from syntax-platform for downstream convenience.
pub use syntax_platform::PreparedFile;

// ---------------------------------------------------------------------------
// Pipeline error
// ---------------------------------------------------------------------------

/// Unified error type for the indexing pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// An error during the discovery stage.
    Discovery(repo_walker::WalkError),
    /// An I/O error reading file content.
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// A persistence error from the metadata store.
    Persist(store::StoreError),
    /// A record failed validation.
    Validation(String),
    /// An internal error (e.g. timestamp formatting).
    Internal(String),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovery(e) => write!(f, "discovery error: {e}"),
            Self::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "I/O error at '{}': {source}", path.display())
                } else {
                    write!(f, "I/O error: {source}")
                }
            }
            Self::Persist(e) => write!(f, "persist error: {e}"),
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Discovery(e) => Some(e),
            Self::Io { source, .. } => Some(source),
            Self::Persist(e) => Some(e),
            _ => None,
        }
    }
}

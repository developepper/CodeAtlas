use std::path::{Path, PathBuf};

use crate::dispatch::DispatchContext;
use crate::registry::BackendRegistry;

/// Configuration and shared state for a single pipeline run.
///
/// Constructed once per index invocation and threaded through every stage.
/// The metadata store is **not** part of this context — it is passed
/// separately (as `&mut`) to stages that need write access, keeping the
/// context immutably shareable across read-only stages.
pub struct PipelineContext<'a> {
    /// Unique identifier for the repository being indexed.
    pub repo_id: String,
    /// Absolute path to the repository root on disk.
    pub source_root: PathBuf,
    /// Backend registry for selecting syntax and semantic backends.
    pub registry: &'a dyn BackendRegistry,
    /// Dispatch context controlling which backends are invoked.
    pub dispatch_context: DispatchContext,
    /// Optional correlation ID for structured log tracing.
    pub correlation_id: Option<String>,
    /// When `true`, use git-diff to accelerate change detection on
    /// git-backed repositories. Falls back to hash-based detection
    /// when the repository is not a git repo or git is unavailable.
    pub use_git_diff: bool,
}

impl<'a> PipelineContext<'a> {
    /// Returns the source root as a [`Path`] reference.
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
}

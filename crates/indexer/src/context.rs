use std::path::{Path, PathBuf};

use adapter_api::{AdapterPolicy, AdapterRouter, IndexContext};

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
    /// Adapter router for selecting language adapters.
    pub router: &'a dyn AdapterRouter,
    /// Default adapter selection policy applied when no per-language override
    /// is configured.
    pub default_policy: AdapterPolicy,
    /// Optional correlation ID for structured log tracing.
    pub correlation_id: Option<String>,
    /// When `true`, use git-diff to accelerate change detection on
    /// git-backed repositories. Falls back to hash-based detection
    /// when the repository is not a git repo or git is unavailable.
    pub use_git_diff: bool,
}

impl<'a> PipelineContext<'a> {
    /// Builds the [`IndexContext`] passed to adapter invocations.
    pub fn index_context(&self) -> IndexContext {
        IndexContext {
            repo_id: self.repo_id.clone(),
            source_root: self.source_root.clone(),
        }
    }

    /// Returns the source root as a [`Path`] reference.
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
}

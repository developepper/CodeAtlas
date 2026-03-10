//! CLI error types.

use std::fmt;

/// Unified error type for CLI commands.
#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Index(indexer::PipelineError),
    Query(query_engine::QueryError),
    Store(store::StoreError),
    Io(std::io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "{msg}"),
            Self::Index(e) => write!(f, "indexing failed: {e}"),
            Self::Query(e) => write!(f, "query failed: {e}"),
            Self::Store(e) => write!(f, "store error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Index(e) => Some(e),
            Self::Query(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Usage(_) => None,
        }
    }
}

impl From<indexer::PipelineError> for CliError {
    fn from(e: indexer::PipelineError) -> Self {
        Self::Index(e)
    }
}

impl From<query_engine::QueryError> for CliError {
    fn from(e: query_engine::QueryError) -> Self {
        Self::Query(e)
    }
}

impl From<store::StoreError> for CliError {
    fn from(e: store::StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

//! MCP request/response envelope types.

use serde::{Deserialize, Serialize};

/// Unified MCP tool response envelope.
///
/// Every tool returns this structure with either a `"success"` or `"error"` status.
/// The `_meta` field carries timing, truncation, and quality metadata per spec §10.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResponse {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
    pub _meta: McpMeta,
}

/// Envelope metadata returned with every MCP tool response (spec §10.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpMeta {
    /// Wall-clock time for the tool call in milliseconds.
    pub timing_ms: u64,
    /// Whether the result set was truncated by a limit.
    pub truncated: bool,
    /// Semantic/syntax quality mix of the returned results.
    pub quality_stats: QualityStats,
    /// Schema version of the index that served the query.
    pub index_version: String,
}

/// Quality mix statistics for MCP result sets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityStats {
    pub semantic_percent: f64,
    pub syntax_percent: f64,
}

impl Default for QualityStats {
    fn default() -> Self {
        Self {
            semantic_percent: 0.0,
            syntax_percent: 100.0,
        }
    }
}

impl Default for McpMeta {
    fn default() -> Self {
        Self {
            timing_ms: 0,
            truncated: false,
            quality_stats: QualityStats::default(),
            index_version: core_model::schema_version::current_index_schema_version().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Success,
    Error,
}

/// Structured error payload for MCP tool responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

/// Error codes for MCP tool failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidParams,
    NotFound,
    StoreError,
    UnknownTool,
    Internal,
}

/// Output returned by tool handlers to the registry.
///
/// Carries the serialized JSON payload, whether the result set was truncated,
/// and the computed quality mix from the actual records returned, so the
/// registry can populate `_meta` with accurate fields.
pub struct ToolOutput {
    pub payload: serde_json::Value,
    pub truncated: bool,
    pub quality_stats: QualityStats,
}

/// Convenience result type for tool handlers.
pub type ToolResult = Result<ToolOutput, McpError>;

impl McpResponse {
    /// Create a success response with the given payload and metadata.
    pub fn success(payload: serde_json::Value, meta: McpMeta) -> Self {
        Self {
            status: Status::Success,
            payload: Some(payload),
            error: None,
            _meta: meta,
        }
    }

    /// Create an error response with the given error and metadata.
    pub fn error(err: McpError, meta: McpMeta) -> Self {
        Self {
            status: Status::Error,
            payload: None,
            error: Some(err),
            _meta: meta,
        }
    }
}

impl McpError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::NotFound,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn store_error(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::StoreError,
            message: message.into(),
            retryable: true,
        }
    }

    pub fn unknown_tool(name: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::UnknownTool,
            message: name.into(),
            retryable: false,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Internal,
            message: message.into(),
            retryable: false,
        }
    }
}

/// Maps a [`query_engine::QueryError`] to an [`McpError`].
impl From<query_engine::QueryError> for McpError {
    fn from(e: query_engine::QueryError) -> Self {
        match &e {
            query_engine::QueryError::EmptyQuery => McpError::invalid_params(e.to_string()),
            query_engine::QueryError::NotFound { .. } => McpError::not_found(e.to_string()),
            query_engine::QueryError::Store(_) => McpError::store_error(e.to_string()),
        }
    }
}

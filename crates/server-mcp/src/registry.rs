//! MCP tool registry — dispatches tool calls by name to query-engine handlers.

use std::time::Instant;

use query_engine::QueryService;
use serde_json::Value;
use tracing::info_span;

use crate::tools;
use crate::types::{McpError, McpMeta, McpResponse};

/// Tool names recognized by the registry.
pub const TOOL_NAMES: &[&str] = &[
    "search_symbols",
    "get_symbol",
    "get_symbols",
    "get_file_outline",
    "get_file_content",
    "get_file_tree",
    "get_repo_outline",
    "search_text",
];

/// Central dispatcher that routes MCP tool calls to query-engine handlers.
///
/// Holds a reference to a [`QueryService`] implementation and dispatches
/// incoming requests by tool name.
pub struct ToolRegistry<'a> {
    svc: &'a dyn QueryService,
}

impl<'a> ToolRegistry<'a> {
    pub fn new(svc: &'a dyn QueryService) -> Self {
        Self { svc }
    }

    /// Returns the list of registered tool names.
    pub fn tool_names(&self) -> &'static [&'static str] {
        TOOL_NAMES
    }

    /// Dispatch a tool call by name with the given JSON parameters.
    ///
    /// Returns an [`McpResponse`] with either a success payload or a
    /// structured error. Unknown tool names produce an error response.
    /// Every response includes `_meta` with timing, truncation, and quality stats.
    pub fn call(&self, tool_name: &str, params: Value) -> McpResponse {
        let span = info_span!("mcp_tool_call", tool = %tool_name);
        let _guard = span.enter();

        let start = Instant::now();

        let result = match tool_name {
            "search_symbols" => tools::search_symbols(self.svc, params),
            "get_symbol" => tools::get_symbol(self.svc, params),
            "get_symbols" => tools::get_symbols(self.svc, params),
            "get_file_outline" => tools::get_file_outline(self.svc, params),
            "get_file_content" => tools::get_file_content(self.svc, params),
            "get_file_tree" => tools::get_file_tree(self.svc, params),
            "get_repo_outline" => tools::get_repo_outline(self.svc, params),
            "search_text" => tools::search_text(self.svc, params),
            _ => {
                let meta = McpMeta {
                    timing_ms: start.elapsed().as_millis() as u64,
                    ..McpMeta::default()
                };
                return McpResponse::error(
                    McpError::unknown_tool(format!("unknown tool: {tool_name}")),
                    meta,
                );
            }
        };

        let timing_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                let meta = McpMeta {
                    timing_ms,
                    truncated: output.truncated,
                    quality_stats: output.quality_stats,
                    ..McpMeta::default()
                };
                McpResponse::success(output.payload, meta)
            }
            Err(err) => {
                let meta = McpMeta {
                    timing_ms,
                    ..McpMeta::default()
                };
                McpResponse::error(err, meta)
            }
        }
    }
}

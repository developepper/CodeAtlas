//! MCP tool server for CodeAtlas.
//!
//! Provides a tool registry that maps MCP tool names to query-engine
//! endpoints, returning structured success/error payloads with `_meta`
//! envelope per spec §10.1.

pub mod registry;
pub mod tools;
pub mod types;

pub use registry::ToolRegistry;
pub use types::{McpError, McpMeta, McpResponse, ToolResult};

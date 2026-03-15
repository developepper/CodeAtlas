//! Stdio MCP transport — newline-delimited JSON-RPC over stdin/stdout.
//!
//! Per MCP spec 2025-11-25, stdio messages are delimited by newlines and
//! MUST NOT contain embedded newlines. This module implements the MCP
//! protocol subset for tool serving: `initialize`,
//! `notifications/initialized`, `tools/list`, `tools/call`, and `ping`.
//!
//! Compatibility shims are included for methods that documented MCP clients
//! (Claude Desktop, Cursor, OpenAI Codex CLI) may probe during startup:
//! `resources/list` and `prompts/list` return empty lists rather than
//! `METHOD_NOT_FOUND`, and `notifications/cancelled` is silently accepted.
//! These shims keep the server generic and do not add client-specific
//! branching. See inline comments marked `COMPAT:` for rationale.
//!
//! All protocol output goes to stdout. All diagnostics go to stderr.

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use server_mcp::ToolRegistry;

// ── Signal handling ──────────────────────────────────────────────────

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install signal handlers for SIGTERM and SIGINT that set an atomic flag.
///
/// The serve loop checks this flag each iteration and exits cleanly.
/// This prevents partial writes to stdout on interruption.
///
/// Must be called once before [`serve`], typically from the CLI entry point.
/// Kept separate from `serve` so that in-process unit tests (which share a
/// process and run in parallel threads) are not affected by global signal
/// state.
pub fn install_signal_handlers() {
    unsafe {
        libc::signal(
            libc::SIGTERM,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            signal_handler as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn signal_handler(_sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

// ── JSON-RPC types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    result: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorObj {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: JsonRpcErrorObj,
}

// Standard JSON-RPC 2.0 error codes.
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

// ── Newline-delimited framing ──────────────────────────────────────────

/// Read one newline-delimited JSON message from `reader`.
///
/// Returns `Ok(None)` on clean EOF. Skips blank lines between messages.
/// Detects and rejects Content-Length framed input with a clear error.
fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|e| format!("read error: {e}"))?;

        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue; // Skip blank lines between messages.
        }

        // Detect Content-Length framed input (2024-11-05 transport).
        if trimmed.starts_with("Content-Length:") {
            return Err(
                "received Content-Length header; this server uses newline-delimited \
                 JSON per MCP spec 2025-11-25, not Content-Length framing"
                    .into(),
            );
        }

        return Ok(Some(trimmed.to_string()));
    }
}

/// Write one newline-delimited JSON message to `writer`.
///
/// `serde_json::to_string` produces compact JSON without embedded newlines,
/// so the output is always a single line as the spec requires.
fn write_message(writer: &mut impl Write, json: &str) -> std::io::Result<()> {
    writeln!(writer, "{json}")?;
    writer.flush()
}

// ── Helpers for writing responses ──────────────────────────────────────

fn write_result(writer: &mut impl Write, id: Value, result: Value) -> Result<(), String> {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result,
    };
    let json = serde_json::to_string(&resp).map_err(|e| format!("serialization error: {e}"))?;
    write_message(writer, &json).map_err(|e| format!("write error: {e}"))
}

fn write_error(
    writer: &mut impl Write,
    id: Value,
    code: i32,
    message: String,
) -> Result<(), String> {
    let resp = JsonRpcErrorResponse {
        jsonrpc: "2.0",
        id,
        error: JsonRpcErrorObj {
            code,
            message,
            data: None,
        },
    };
    let json = serde_json::to_string(&resp).map_err(|e| format!("serialization error: {e}"))?;
    write_message(writer, &json).map_err(|e| format!("write error: {e}"))
}

// ── MCP server loop ───────────────────────────────────────────────────

/// Run the MCP stdio server, reading from `input` and writing to `output`.
///
/// The loop terminates on clean EOF (stdin close). Parse errors produce
/// JSON-RPC error responses; the server continues processing.
pub fn serve(registry: &ToolRegistry, input: impl Read, output: impl Write) -> Result<(), String> {
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    loop {
        // ── Check for signal-based shutdown ────────────────────────
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            eprintln!("codeatlas mcp: received signal, shutting down");
            return Ok(());
        }

        // ── Read message ───────────────────────────────────────────
        let message = match read_message(&mut reader) {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                eprintln!("codeatlas mcp: stdin closed, shutting down");
                return Ok(());
            }
            Err(e) => {
                eprintln!("codeatlas mcp: framing error: {e}");
                write_error(&mut writer, Value::Null, PARSE_ERROR, e)?;
                continue;
            }
        };

        // ── Parse JSON-RPC request ─────────────────────────────────
        let request: JsonRpcRequest = match serde_json::from_str(&message) {
            Ok(req) => req,
            Err(e) => {
                write_error(
                    &mut writer,
                    Value::Null,
                    PARSE_ERROR,
                    format!("invalid JSON: {e}"),
                )?;
                continue;
            }
        };

        // ── Validate jsonrpc version ───────────────────────────────
        if request.jsonrpc != "2.0" {
            write_error(
                &mut writer,
                request.id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "jsonrpc must be \"2.0\"".into(),
            )?;
            continue;
        }

        let is_notification = request.id.is_none();

        // ── Route by method ────────────────────────────────────────
        match request.method.as_str() {
            "initialize" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, handle_initialize())?;
            }
            "notifications/initialized" => {
                // Notification — no response required.
                eprintln!("codeatlas mcp: client initialized");
            }
            "tools/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, handle_tools_list(registry))?;
            }
            "tools/call" => {
                let id = request.id.unwrap_or(Value::Null);
                match handle_tools_call(registry, request.params) {
                    Ok(result) => write_result(&mut writer, id, result)?,
                    Err(err) => write_error(&mut writer, id, err.code, err.message)?,
                }
            }
            // COMPAT: MCP clients may send `ping` as a health check.
            // Returning an empty result keeps the connection alive.
            "ping" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({}))?;
            }
            // COMPAT: Some MCP clients probe `resources/list` and
            // `prompts/list` during startup to discover server capabilities.
            // Returning empty lists is more interoperable than METHOD_NOT_FOUND,
            // which can cause some clients to treat the server as unhealthy.
            "resources/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({ "resources": [] }))?;
            }
            "prompts/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({ "prompts": [] }))?;
            }
            // COMPAT: Clients may send `notifications/cancelled` when aborting
            // a request. This is a notification (no id) and requires no response.
            "notifications/cancelled" => {
                // Silently accepted — no response required.
            }
            _ => {
                if !is_notification {
                    let id = request.id.unwrap_or(Value::Null);
                    write_error(
                        &mut writer,
                        id,
                        METHOD_NOT_FOUND,
                        format!("method not found: {}", request.method),
                    )?;
                }
                // Unknown notifications are silently ignored per JSON-RPC.
            }
        }
    }
}

// ── MCP method handlers ───────────────────────────────────────────────

fn handle_initialize() -> Value {
    serde_json::json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "codeatlas",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn handle_tools_list(registry: &ToolRegistry) -> Value {
    let tools: Vec<Value> = registry
        .tool_names()
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": tool_description(name),
                "inputSchema": tool_input_schema(name)
            })
        })
        .collect();

    serde_json::json!({ "tools": tools })
}

fn handle_tools_call(
    registry: &ToolRegistry,
    params: Option<Value>,
) -> Result<Value, JsonRpcErrorObj> {
    let params = params.ok_or_else(|| JsonRpcErrorObj {
        code: INVALID_PARAMS,
        message: "missing params".into(),
        data: None,
    })?;

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcErrorObj {
            code: INVALID_PARAMS,
            message: "missing or invalid 'name' in params".into(),
            data: None,
        })?;

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let mcp_response = registry.call(name, arguments);

    let text = serde_json::to_string(&mcp_response).map_err(|e| JsonRpcErrorObj {
        code: INTERNAL_ERROR,
        message: format!("failed to serialize tool response: {e}"),
        data: None,
    })?;

    let is_error = mcp_response.error.is_some();

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "isError": is_error
    }))
}

pub(crate) fn tool_description(name: &str) -> &'static str {
    match name {
        "search_symbols" => "Search for symbols by name with optional filters",
        "get_symbol" => "Get a symbol by its unique ID",
        "get_symbols" => "Get multiple symbols by their IDs",
        "get_file_outline" => "List symbols defined in a file",
        "get_file_content" => "Get the content of an indexed file",
        "get_file_tree" => "List files in a repository or subtree",
        "get_repo_outline" => "Show repository structure and file summary",
        "search_text" => "Search for text patterns across indexed files",
        "list_repos" => "List all indexed repositories with status and metadata",
        "get_repo_status" => "Get detailed status and metadata for a specific repository",
        _ => "CodeAtlas tool",
    }
}

/// Return a JSON Schema `inputSchema` for the given tool name.
///
/// Each schema matches the corresponding `*Params` struct in
/// `crates/server-mcp/src/tools.rs`. Required fields mirror non-`Option`
/// struct fields; optional fields and those with `#[serde(default)]` are
/// listed only in `properties`.
pub(crate) fn tool_input_schema(name: &str) -> Value {
    match name {
        "search_symbols" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                },
                "query": {
                    "type": "string",
                    "description": "Search query string for symbol names"
                },
                "kind": {
                    "type": "string",
                    "description": "Filter by symbol kind",
                    "enum": ["function", "class", "method", "type", "constant"]
                },
                "language": {
                    "type": "string",
                    "description": "Filter by programming language"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "minimum": 0,
                    "default": 20
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of results to skip for pagination",
                    "minimum": 0,
                    "default": 0
                }
            },
            "required": ["repo_id", "query"]
        }),

        "get_symbol" => serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Unique symbol identifier"
                }
            },
            "required": ["id"]
        }),

        "get_symbols" => serde_json::json!({
            "type": "object",
            "properties": {
                "ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of unique symbol identifiers"
                }
            },
            "required": ["ids"]
        }),

        "get_file_outline" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path of the file within the repository"
                }
            },
            "required": ["repo_id", "file_path"]
        }),

        "get_file_content" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path of the file within the repository"
                }
            },
            "required": ["repo_id", "file_path"]
        }),

        "get_file_tree" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                },
                "path_prefix": {
                    "type": "string",
                    "description": "Filter files by path prefix (subtree)"
                }
            },
            "required": ["repo_id"]
        }),

        "get_repo_outline" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                }
            },
            "required": ["repo_id"]
        }),

        "search_text" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                },
                "pattern": {
                    "type": "string",
                    "description": "Text search pattern"
                },
                "kind": {
                    "type": "string",
                    "description": "Filter by symbol kind",
                    "enum": ["function", "class", "method", "type", "constant"]
                },
                "language": {
                    "type": "string",
                    "description": "Filter by programming language"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "minimum": 0,
                    "default": 20
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of results to skip for pagination",
                    "minimum": 0,
                    "default": 0
                }
            },
            "required": ["repo_id", "pattern"]
        }),

        "list_repos" => serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),

        "get_repo_status" => serde_json::json!({
            "type": "object",
            "properties": {
                "repo_id": {
                    "type": "string",
                    "description": "Repository identifier"
                }
            },
            "required": ["repo_id"]
        }),

        _ => serde_json::json!({
            "type": "object"
        }),
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Framing tests ──────────────────────────────────────────────

    #[test]
    fn read_message_valid() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
        let mut reader = std::io::BufReader::new(&input[..]);
        let result = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(result, r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    }

    #[test]
    fn read_message_eof_returns_none() {
        let mut reader = std::io::BufReader::new(&b""[..]);
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn read_message_skips_blank_lines() {
        let input = b"\n\n{\"ok\":true}\n";
        let mut reader = std::io::BufReader::new(&input[..]);
        let result = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(result, r#"{"ok":true}"#);
    }

    #[test]
    fn read_message_rejects_content_length() {
        let input = b"Content-Length: 42\r\n";
        let mut reader = std::io::BufReader::new(&input[..]);
        let err = read_message(&mut reader).unwrap_err();
        assert!(err.contains("Content-Length"));
        assert!(err.contains("newline-delimited"));
    }

    #[test]
    fn read_message_multiple() {
        let input = b"{\"id\":1}\n{\"id\":2}\n";
        let mut reader = std::io::BufReader::new(&input[..]);
        assert_eq!(read_message(&mut reader).unwrap().unwrap(), r#"{"id":1}"#);
        assert_eq!(read_message(&mut reader).unwrap().unwrap(), r#"{"id":2}"#);
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn write_message_format() {
        let mut buf = Vec::new();
        write_message(&mut buf, r#"{"test":true}"#).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output, "{\"test\":true}\n");
    }

    // ── Request parsing tests ──────────────────────────────────────

    #[test]
    fn parse_request_with_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"id":"test"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
        assert!(req.params.is_some());
        assert_eq!(req.id, Some(Value::Number(1.into())));
    }

    #[test]
    fn parse_notification_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(req.id.is_none());
        assert!(req.params.is_none());
    }

    // ── Response serialization tests ───────────────────────────────

    #[test]
    fn serialize_success_response() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::Number(1.into()),
            result: serde_json::json!({"ok": true}),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["result"]["ok"], true);
    }

    #[test]
    fn serialize_error_response() {
        let resp = JsonRpcErrorResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            error: JsonRpcErrorObj {
                code: PARSE_ERROR,
                message: "bad input".into(),
                data: None,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["code"], PARSE_ERROR);
        assert_eq!(parsed["error"]["message"], "bad input");
    }

    // ── Server loop integration tests (in-process) ─────────────────

    /// Helper: build newline-delimited input from a list of JSON bodies.
    fn messages(bodies: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for body in bodies {
            writeln!(buf, "{body}").unwrap();
        }
        buf
    }

    /// Helper: parse all newline-delimited responses from output bytes.
    fn parse_responses(output: &[u8]) -> Vec<Value> {
        let mut reader = std::io::BufReader::new(output);
        let mut responses = Vec::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        responses.push(serde_json::from_str(trimmed).unwrap());
                    }
                }
                Err(_) => break,
            }
        }
        responses
    }

    /// Create a ToolRegistry backed by an empty in-memory DB.
    fn test_registry() -> (
        store::MetadataStore,
        impl Fn(&store::MetadataStore) -> ToolRegistry,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = store::MetadataStore::open(&db_path).unwrap();
        let blob_dir = tempfile::tempdir().unwrap();
        let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
        // Leak dirs and blob store to keep them alive for the test duration.
        std::mem::forget(dir);
        std::mem::forget(blob_dir);
        let blob_store_ref: &'static store::BlobStore = Box::leak(Box::new(blob_store));
        (db, move |db: &store::MetadataStore| {
            let svc = Box::new(query_engine::StoreQueryService::new(db, blob_store_ref));
            let svc_ptr = Box::into_raw(svc);
            // SAFETY: the heap allocation is intentionally leaked so the
            // ToolRegistry reference remains valid for the test duration.
            // The inner &db reference is valid because the caller keeps
            // the MetadataStore alive in the returned tuple.
            let svc_ref: &'static dyn query_engine::QueryService =
                unsafe { std::mem::transmute(&*svc_ptr as &dyn query_engine::QueryService) };
            ToolRegistry::new(svc_ref)
        })
    }

    #[test]
    fn serve_initialize_handshake() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let r = &responses[0];
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "codeatlas");
    }

    #[test]
    fn serve_tools_list() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let tools = responses[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"search_symbols"));
        assert!(names.contains(&"get_symbol"));
        assert!(names.contains(&"get_file_tree"));

        for tool in tools {
            assert!(tool["description"].is_string());
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object");
            assert!(
                schema["properties"].is_object(),
                "tool {} missing properties",
                tool["name"]
            );
            assert!(
                schema["required"].is_array(),
                "tool {} missing required",
                tool["name"]
            );
        }
    }

    // ── Schema content tests ──────────────────────────────────────────

    #[test]
    fn schema_search_symbols_matches_params() {
        let schema = tool_input_schema("search_symbols");
        let props = schema["properties"].as_object().unwrap();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        // Required fields from SearchSymbolsParams
        assert!(required.contains(&"repo_id"));
        assert!(required.contains(&"query"));
        assert_eq!(required.len(), 2);

        // All properties present
        assert!(props.contains_key("repo_id"));
        assert!(props.contains_key("query"));
        assert!(props.contains_key("kind"));
        assert!(props.contains_key("language"));
        assert!(props.contains_key("limit"));
        assert!(props.contains_key("offset"));
        assert_eq!(props.len(), 6);

        // kind has enum constraint
        assert!(schema["properties"]["kind"]["enum"].is_array());

        // limit/offset have minimum and default matching usize semantics
        assert_eq!(schema["properties"]["limit"]["default"], 20);
        assert_eq!(schema["properties"]["limit"]["minimum"], 0);
        assert_eq!(schema["properties"]["offset"]["default"], 0);
        assert_eq!(schema["properties"]["offset"]["minimum"], 0);
    }

    #[test]
    fn schema_get_symbol_matches_params() {
        let schema = tool_input_schema("get_symbol");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["id"]);
        assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn schema_get_symbols_matches_params() {
        let schema = tool_input_schema("get_symbols");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["ids"]);
        assert_eq!(schema["properties"]["ids"]["type"], "array");
        assert_eq!(schema["properties"]["ids"]["items"]["type"], "string");
    }

    #[test]
    fn schema_file_outline_matches_params() {
        let schema = tool_input_schema("get_file_outline");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"repo_id"));
        assert!(required.contains(&"file_path"));
        assert_eq!(required.len(), 2);
    }

    #[test]
    fn schema_file_content_matches_params() {
        let schema = tool_input_schema("get_file_content");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"repo_id"));
        assert!(required.contains(&"file_path"));
        assert_eq!(required.len(), 2);
    }

    #[test]
    fn schema_file_tree_matches_params() {
        let schema = tool_input_schema("get_file_tree");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["repo_id"]);
        // path_prefix is optional
        assert!(schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("path_prefix"));
        assert_eq!(schema["properties"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn schema_repo_outline_matches_params() {
        let schema = tool_input_schema("get_repo_outline");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, vec!["repo_id"]);
        assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn schema_search_text_matches_params() {
        let schema = tool_input_schema("search_text");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"repo_id"));
        assert!(required.contains(&"pattern"));
        assert_eq!(required.len(), 2);
        assert_eq!(schema["properties"].as_object().unwrap().len(), 6);
        assert!(schema["properties"]["kind"]["enum"].is_array());

        // limit/offset have minimum matching usize semantics
        assert_eq!(schema["properties"]["limit"]["minimum"], 0);
        assert_eq!(schema["properties"]["offset"]["minimum"], 0);
    }

    #[test]
    fn schema_all_tools_covered() {
        // Verify every registered tool has a non-stub schema.
        for name in server_mcp::registry::TOOL_NAMES {
            let schema = tool_input_schema(name);
            assert!(
                schema["properties"].is_object(),
                "tool {name} has no properties in schema"
            );
            assert!(
                schema["required"].is_array(),
                "tool {name} has no required field in schema"
            );
        }
    }

    #[test]
    fn schema_names_match_registry() {
        // Verify tool_input_schema and tool_description cover all registry names.
        for name in server_mcp::registry::TOOL_NAMES {
            let desc = tool_description(name);
            assert_ne!(desc, "CodeAtlas tool", "tool {name} has no description");
        }
    }

    #[test]
    fn schema_does_not_set_additional_properties() {
        // The param structs do not use #[serde(deny_unknown_fields)], so the
        // schema must not claim additionalProperties: false. That would be
        // stricter than the runtime and break clients that validate inputs.
        for name in server_mcp::registry::TOOL_NAMES {
            let schema = tool_input_schema(name);
            assert!(
                schema.get("additionalProperties").is_none(),
                "tool {name} must not set additionalProperties"
            );
        }
    }

    #[test]
    fn schema_limit_offset_have_minimum() {
        // usize fields reject negative values at deserialization. The schema
        // must advertise minimum: 0 so clients don't send invalid values.
        for name in &["search_symbols", "search_text"] {
            let schema = tool_input_schema(name);
            for field in &["limit", "offset"] {
                assert_eq!(
                    schema["properties"][field]["minimum"], 0,
                    "{name}.{field} must have minimum: 0"
                );
            }
        }
    }

    #[test]
    fn runtime_accepts_unknown_fields() {
        // Confirms that the runtime does not reject unknown fields, which is
        // why the schema must not set additionalProperties: false.
        use server_mcp::tools::SearchSymbolsParams;
        let json = serde_json::json!({
            "repo_id": "test",
            "query": "foo",
            "totally_unknown_field": true
        });
        let result: Result<SearchSymbolsParams, _> = serde_json::from_value(json);
        assert!(result.is_ok(), "serde should accept unknown fields");
    }

    #[test]
    fn runtime_rejects_negative_limit() {
        // Confirms that the runtime rejects negative values for usize fields,
        // which is why the schema must set minimum: 0.
        use server_mcp::tools::SearchSymbolsParams;
        let json = serde_json::json!({
            "repo_id": "test",
            "query": "foo",
            "limit": -1
        });
        let result: Result<SearchSymbolsParams, _> = serde_json::from_value(json);
        assert!(result.is_err(), "serde should reject negative usize");
    }

    #[test]
    fn serve_tools_call_unknown_tool() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nonexistent","arguments":{}}}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let r = &responses[0];
        assert!(r.get("result").is_some());
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn serve_tools_call_missing_params() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/call"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn serve_unknown_method() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"completions/complete"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn serve_unknown_notification_silent() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","method":"notifications/unknown"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert!(responses.is_empty());
    }

    // ── Compatibility shim tests ────────────────────────────────────

    #[test]
    fn serve_ping_returns_empty_result() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let r = &responses[0];
        assert_eq!(r["id"], 1);
        assert!(r["result"].is_object());
        assert!(r.get("error").is_none());
    }

    #[test]
    fn serve_resources_list_returns_empty() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let r = &responses[0];
        assert_eq!(r["id"], 1);
        let resources = r["result"]["resources"].as_array().unwrap();
        assert!(resources.is_empty());
    }

    #[test]
    fn serve_prompts_list_returns_empty() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"prompts/list"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let r = &responses[0];
        assert_eq!(r["id"], 1);
        let prompts = r["result"]["prompts"].as_array().unwrap();
        assert!(prompts.is_empty());
    }

    #[test]
    fn serve_notifications_cancelled_silent() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert!(
            responses.is_empty(),
            "notifications/cancelled should produce no response"
        );
    }

    #[test]
    fn serve_tools_list_with_cursor_param() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        // Some clients send cursor: null when requesting the first page.
        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":null}}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let tools = responses[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);
    }

    #[test]
    fn serve_initialize_with_extra_capabilities() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        // Clients may advertise capabilities the server doesn't support.
        // The server should accept and respond normally.
        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true},"sampling":{}},"clientInfo":{"name":"cursor","version":"0.50"}}}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(responses[0]["result"]["serverInfo"]["name"], "codeatlas");
    }

    // ── Remaining server loop tests ──────────────────────────────────

    #[test]
    fn serve_malformed_json() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[
            "not json at all",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], PARSE_ERROR);
        assert!(responses[1]["result"]["tools"].is_array());
    }

    #[test]
    fn serve_invalid_jsonrpc_version() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"1.0","id":1,"method":"initialize"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn serve_invalid_jsonrpc_version_no_id() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[r#"{"jsonrpc":"1.0","method":"initialize"}"#]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["error"]["code"], INVALID_REQUEST);
        assert!(responses[0]["id"].is_null());
    }

    #[test]
    fn serve_content_length_rejected() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let mut input = Vec::new();
        // Content-Length framed message (wrong transport format).
        writeln!(input, "Content-Length: 47").unwrap();
        writeln!(input).unwrap();
        write!(input, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize"}}"#).unwrap();
        // Followed by a valid newline-delimited message.
        writeln!(input).unwrap();
        writeln!(input, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#).unwrap();

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert!(!responses.is_empty());
        assert_eq!(responses[0]["error"]["code"], PARSE_ERROR);
        let msg = responses[0]["error"]["message"].as_str().unwrap();
        assert!(msg.contains("Content-Length"));
    }

    #[test]
    fn serve_full_handshake_and_tool_call() {
        let (db, make_registry) = test_registry();
        let registry = make_registry(&db);

        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_file_tree","arguments":{"repo_id":"nonexistent"}}}"#,
        ]);

        let mut output = Vec::new();
        serve(&registry, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 3);

        // initialize
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");

        // tools/list
        assert_eq!(responses[1]["id"], 2);
        assert!(responses[1]["result"]["tools"].is_array());

        // tools/call — repo doesn't exist, but that's a tool-level error.
        assert_eq!(responses[2]["id"], 3);
        assert!(responses[2]["result"]["content"].is_array());
    }
}

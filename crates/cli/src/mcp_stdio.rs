//! Stdio MCP transport — newline-delimited JSON-RPC over stdin/stdout.
//!
//! Per MCP spec 2025-11-25, stdio messages are delimited by newlines and
//! MUST NOT contain embedded newlines. This module implements the minimal
//! MCP protocol subset for tool serving: `initialize`,
//! `notifications/initialized`, `tools/list`, and `tools/call`.
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
                "inputSchema": { "type": "object" }
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

fn tool_description(name: &str) -> &'static str {
    match name {
        "search_symbols" => "Search for symbols by name with optional filters",
        "get_symbol" => "Get a symbol by its unique ID",
        "get_symbols" => "Get multiple symbols by their IDs",
        "get_file_outline" => "List symbols defined in a file",
        "get_file_content" => "Get the content of an indexed file",
        "get_file_tree" => "List files in a repository or subtree",
        "get_repo_outline" => "Show repository structure and file summary",
        "search_text" => "Search for text patterns across indexed files",
        _ => "CodeAtlas tool",
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
        // Leak dir to keep the temp directory alive for the test duration.
        std::mem::forget(dir);
        (db, |db: &store::MetadataStore| {
            let svc = Box::new(query_engine::StoreQueryService::new(db));
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
        assert_eq!(tools.len(), 8);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"search_symbols"));
        assert!(names.contains(&"get_symbol"));
        assert!(names.contains(&"get_file_tree"));

        for tool in tools {
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"].is_object());
        }
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

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#]);

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

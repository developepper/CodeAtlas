//! MCP-to-HTTP bridge — translates stdio MCP tool calls into HTTP
//! requests to the running CodeAtlas service.
//!
//! This module implements the same MCP protocol subset as `mcp_stdio`
//! but instead of dispatching tools through a local `ToolRegistry`, it
//! forwards `tools/call` requests to the persistent HTTP service via
//! `POST /tools/call`.
//!
//! All protocol output goes to stdout. All diagnostics go to stderr.
//!
//! ## Technical debt
//!
//! The JSON-RPC types, message framing, and MCP protocol handling are
//! duplicated from `mcp_stdio.rs`. A future cleanup should extract the
//! shared MCP protocol machinery (types, framing, initialize/ping/shim
//! handlers) into a common module that both the direct server and the
//! bridge can use.

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use server_mcp::registry::TOOL_NAMES;

use crate::mcp_stdio;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install signal handlers for the bridge process.
#[cfg(unix)]
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

/// No-op on non-Unix platforms. Signal-based shutdown relies on stdin
/// EOF from the client process instead.
#[cfg(not(unix))]
pub fn install_signal_handlers() {}

#[cfg(unix)]
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

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

// ── Newline-delimited framing ──────────────────────────────────────────

fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|e| format!("read error: {e}"))?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

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

fn write_message(writer: &mut impl Write, json: &str) -> std::io::Result<()> {
    writeln!(writer, "{json}")?;
    writer.flush()
}

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

// ── HTTP client ────────────────────────────────────────────────────────

/// Minimal HTTP client for communicating with the local service.
struct ServiceClient {
    addr: String,
}

impl ServiceClient {
    fn new(addr: String) -> Self {
        Self { addr }
    }

    /// POST JSON to `path` and return `(status_code, body)`.
    fn post(&self, path: &str, body: &str) -> Result<(u16, String), String> {
        let mut stream = TcpStream::connect(&self.addr)
            .map_err(|e| format!("cannot connect to service at {}: {e}", self.addr))?;
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();

        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.addr,
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| format!("read error: {e}"))?;

        // Parse status code from "HTTP/1.1 200 OK\r\n..."
        let status_code = response
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);

        // Extract body after the blank line.
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .map(|s| s.to_string())
            .ok_or_else(|| "malformed HTTP response".to_string())?;

        Ok((status_code, body))
    }

    /// Check if the service is reachable.
    fn health_check(&self) -> Result<(), String> {
        let mut stream = TcpStream::connect(&self.addr)
            .map_err(|e| format!("cannot connect to service at {}: {e}", self.addr))?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let request = format!(
            "GET /health HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.addr
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| format!("read error: {e}"))?;

        if response.contains("200 OK") {
            Ok(())
        } else {
            Err(format!("service health check failed: {response}"))
        }
    }
}

// ── Bridge server loop ─────────────────────────────────────────────────

/// Run the MCP bridge, reading from `input` and writing to `output`.
///
/// Proxies `tools/call` to the HTTP service. Handles `initialize`,
/// `tools/list`, `ping`, and compatibility shims locally.
///
/// `service_addr` is the `host:port` of the running CodeAtlas service.
///
/// Validates that the service is reachable before entering the MCP loop.
/// Returns an error if the service cannot be reached.
pub fn serve_bridge(
    service_addr: &str,
    input: impl Read,
    output: impl Write,
) -> Result<(), String> {
    let client = ServiceClient::new(service_addr.to_string());

    // Validate service connectivity before entering the protocol loop.
    client.health_check().map_err(|e| {
        format!(
            "cannot reach CodeAtlas service at {service_addr}: {e}\n\n\
             Hint: start the service with 'codeatlas serve' first."
        )
    })?;

    eprintln!("codeatlas mcp bridge: connected to service at {service_addr}");
    serve(&client, input, output)
}

fn serve(client: &ServiceClient, input: impl Read, output: impl Write) -> Result<(), String> {
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            eprintln!("codeatlas mcp bridge: received signal, shutting down");
            return Ok(());
        }

        let message = match read_message(&mut reader) {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                eprintln!("codeatlas mcp bridge: stdin closed, shutting down");
                return Ok(());
            }
            Err(e) => {
                eprintln!("codeatlas mcp bridge: framing error: {e}");
                write_error(&mut writer, Value::Null, PARSE_ERROR, e)?;
                continue;
            }
        };

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

        match request.method.as_str() {
            "initialize" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, handle_initialize())?;
            }
            "notifications/initialized" => {
                eprintln!("codeatlas mcp bridge: client initialized");
            }
            "tools/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, handle_tools_list())?;
            }
            "tools/call" => {
                let id = request.id.unwrap_or(Value::Null);
                match handle_tools_call(client, request.params) {
                    Ok(result) => write_result(&mut writer, id, result)?,
                    Err(err) => write_error(&mut writer, id, err.code, err.message)?,
                }
            }
            "ping" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({}))?;
            }
            // COMPAT: Same shims as the direct MCP server.
            "resources/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({ "resources": [] }))?;
            }
            "prompts/list" => {
                let id = request.id.unwrap_or(Value::Null);
                write_result(&mut writer, id, serde_json::json!({ "prompts": [] }))?;
            }
            "notifications/cancelled" => {}
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

fn handle_tools_list() -> Value {
    let tools: Vec<Value> = TOOL_NAMES
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": mcp_stdio::tool_description(name),
                "inputSchema": mcp_stdio::tool_input_schema(name)
            })
        })
        .collect();

    serde_json::json!({ "tools": tools })
}

fn handle_tools_call(
    client: &ServiceClient,
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

    // Forward to the HTTP service.
    let request_body = serde_json::json!({
        "name": name,
        "arguments": arguments,
    });

    let body_str = serde_json::to_string(&request_body).map_err(|e| JsonRpcErrorObj {
        code: INTERNAL_ERROR,
        message: format!("failed to serialize request: {e}"),
        data: None,
    })?;

    let (status_code, response_body) =
        client
            .post("/tools/call", &body_str)
            .map_err(|e| JsonRpcErrorObj {
                code: INTERNAL_ERROR,
                message: format!("service request failed: {e}"),
                data: None,
            })?;

    // The service returns different shapes depending on HTTP status:
    // - 200: full MCP response envelope ({"status":..,"payload":..,"_meta":..})
    // - 500: bare infrastructure error ({"error":"..."})
    //
    // On 500 we surface the error as a JSON-RPC INTERNAL_ERROR so clients
    // get the same error shape as if the direct MCP server had an internal
    // failure, rather than wrapping a bare {"error":"..."} as tool content.
    if status_code >= 500 {
        let error_msg = serde_json::from_str::<Value>(&response_body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or_else(|| format!("service returned HTTP {status_code}"));

        return Err(JsonRpcErrorObj {
            code: INTERNAL_ERROR,
            message: format!("service error: {error_msg}"),
            data: None,
        });
    }

    // 200 path: the body is the full MCP response envelope.
    let mcp_response: Value =
        serde_json::from_str(&response_body).map_err(|e| JsonRpcErrorObj {
            code: INTERNAL_ERROR,
            message: format!("failed to parse service response: {e}"),
            data: None,
        })?;

    let is_error = mcp_response.get("error").is_some_and(|e| !e.is_null());

    let text = serde_json::to_string(&mcp_response).map_err(|e| JsonRpcErrorObj {
        code: INTERNAL_ERROR,
        message: format!("failed to serialize tool response: {e}"),
        data: None,
    })?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "isError": is_error
    }))
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(lines: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for line in lines {
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
        }
        buf
    }

    fn parse_responses(output: &[u8]) -> Vec<Value> {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("parse response"))
            .collect()
    }

    #[test]
    fn bridge_initialize_handshake() {
        // Use a dummy client — initialize doesn't hit the service.
        let client = ServiceClient::new("127.0.0.1:0".into());

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(responses[0]["result"]["serverInfo"]["name"], "codeatlas");
    }

    #[test]
    fn bridge_tools_list() {
        let client = ServiceClient::new("127.0.0.1:0".into());

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        let tools = responses[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), TOOL_NAMES.len());

        // Verify all registered tools appear.
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for tool_name in TOOL_NAMES {
            assert!(names.contains(tool_name), "missing tool: {tool_name}");
        }
    }

    #[test]
    fn bridge_ping() {
        let client = ServiceClient::new("127.0.0.1:0".into());

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert!(responses[0]["result"].is_object());
    }

    #[test]
    fn bridge_tools_call_service_unreachable() {
        // Point at a port nothing is listening on.
        let client = ServiceClient::new("127.0.0.1:1".into());

        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_repos","arguments":{}}}"#,
        ]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        // Should get a JSON-RPC error (not crash).
        assert!(responses[0]["error"].is_object());
        assert!(responses[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("service request failed"),);
    }

    #[test]
    fn bridge_compat_shims() {
        let client = ServiceClient::new("127.0.0.1:0".into());

        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"prompts/list"}"#,
        ]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 2);
        assert!(responses[0]["result"]["resources"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(responses[1]["result"]["prompts"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn bridge_unknown_method() {
        let client = ServiceClient::new("127.0.0.1:0".into());

        let input = messages(&[r#"{"jsonrpc":"2.0","id":1,"method":"unknown/method"}"#]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);
        assert!(responses[0]["error"].is_object());
        assert_eq!(responses[0]["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn bridge_tools_call_service_500_returns_jsonrpc_error() {
        // Start a mock HTTP server that always returns 500.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mock_addr = listener.local_addr().unwrap().to_string();

        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Read the request (consume it).
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);

                // Respond with HTTP 500 and a bare error JSON body.
                let body = r#"{"error":"database lock poisoned"}"#;
                let response = format!(
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let client = ServiceClient::new(mock_addr);
        let input = messages(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_repos","arguments":{}}}"#,
        ]);

        let mut output = Vec::new();
        serve(&client, &input[..], &mut output).unwrap();

        handle.join().unwrap();

        let responses = parse_responses(&output);
        assert_eq!(responses.len(), 1);

        // Should be a JSON-RPC error, not a tools/call success wrapping
        // the bare {"error":"..."} as content.
        assert!(
            responses[0]["error"].is_object(),
            "500 should produce a JSON-RPC error, not a result: {:?}",
            responses[0]
        );
        assert_eq!(responses[0]["error"]["code"], INTERNAL_ERROR);
        let msg = responses[0]["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("database lock poisoned"),
            "error should surface the service error message: {msg}"
        );
        // Must NOT have a "result" with "content" — that would mean the
        // bare error was incorrectly wrapped as tool output.
        assert!(responses[0]["result"].is_null());
    }
}

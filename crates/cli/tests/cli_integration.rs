//! Integration tests for the CLI commands.
//!
//! These tests exercise the CLI binary end-to-end by invoking it as a subprocess.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

fn codeatlas_bin() -> PathBuf {
    // Built by `cargo test`, available under target/debug
    let mut path = std::env::current_exe()
        .expect("current_exe")
        .parent()
        .expect("parent")
        .parent()
        .expect("target dir")
        .to_path_buf();
    path.push("codeatlas");
    path
}

fn setup_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "/// A greeting function.\npub fn greet() {}\npub fn helper() {}\n",
    )
    .expect("write lib.rs");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    dir
}

// ---------------------------------------------------------------------------
// Index command tests
// ---------------------------------------------------------------------------

#[test]
fn index_command_succeeds_on_valid_repo() {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("run codeatlas index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("files_discovered:"));
    assert!(stdout.contains("files_with_symbols:"));
    assert!(stdout.contains("symbols_extracted:"));
}

#[test]
fn index_command_fails_on_missing_path() {
    let output = Command::new(codeatlas_bin())
        .args(["index"])
        .output()
        .expect("run codeatlas index");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:") || stderr.contains("error:"));
}

#[test]
fn index_command_fails_on_invalid_path() {
    let output = Command::new(codeatlas_bin())
        .args(["index", "/nonexistent/path/surely"])
        .output()
        .expect("run codeatlas index");

    assert!(!output.status.success());
}

#[test]
fn index_command_fails_on_trailing_db_flag() {
    let output = Command::new(codeatlas_bin())
        .args(["index", "/tmp", "--db"])
        .output()
        .expect("run codeatlas index");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--db requires a value"),
        "should report missing --db value, got: {stderr}"
    );
}

#[test]
fn index_command_fails_on_unknown_flag() {
    let output = Command::new(codeatlas_bin())
        .args(["index", "/tmp", "--verbose"])
        .output()
        .expect("run codeatlas index");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option"),
        "should report unknown option, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Search-symbols command tests
// ---------------------------------------------------------------------------

#[test]
fn search_symbols_finds_indexed_symbol() {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    // First, index the repo.
    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");
    assert!(index_output.status.success(), "index should succeed");

    // Derive repo_id from directory name (same logic as CLI).
    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Search for "greet".
    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "greet",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("greet"),
        "should find greet symbol.\nstdout: {stdout}"
    );
    assert!(stdout.contains("total_candidates:"));
}

#[test]
fn search_symbols_fails_without_required_args() {
    let output = Command::new(codeatlas_bin())
        .args(["search-symbols"])
        .output()
        .expect("search-symbols");

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Get-symbol command tests
// ---------------------------------------------------------------------------

#[test]
fn get_symbol_retrieves_indexed_symbol() {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    // Index the repo.
    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");
    assert!(index_output.status.success());

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // First search to get a symbol ID.
    let search_output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "greet",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search");
    assert!(search_output.status.success());

    let stdout = String::from_utf8_lossy(&search_output.stdout);
    // Extract the symbol ID from output like "  - id: src/lib.rs::greet#function"
    let id_line = stdout
        .lines()
        .find(|l| l.contains("- id:"))
        .expect("should have an id line");
    let symbol_id = id_line.trim().strip_prefix("- id: ").unwrap().trim();

    // Get the symbol by ID.
    let output = Command::new(codeatlas_bin())
        .args(["get-symbol", symbol_id, "--db", db_path.to_str().unwrap()])
        .output()
        .expect("get-symbol");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "get-symbol should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("name: greet"));
    assert!(stdout.contains("kind: function"));
}

#[test]
fn get_symbol_fails_on_unknown_id() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    // Create an empty DB by indexing an empty repo.
    let repo_dir = TempDir::new().expect("repo");
    std::fs::write(repo_dir.path().join("dummy.rs"), "fn x() {}\n").expect("write");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");
    assert!(index_output.status.success());

    let output = Command::new(codeatlas_bin())
        .args([
            "get-symbol",
            "nonexistent-id",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("get-symbol");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

// ---------------------------------------------------------------------------
// Shared helper: index a test repo and return (repo_dir, db_dir, repo_id)
// ---------------------------------------------------------------------------

fn indexed_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");
    assert!(index_output.status.success(), "index should succeed");

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

// ---------------------------------------------------------------------------
// file-outline command tests
// ---------------------------------------------------------------------------

#[test]
fn file_outline_shows_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "src/lib.rs",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("file: src/lib.rs"));
    assert!(stdout.contains("name: greet"));
    assert!(stdout.contains("kind: function"));
}

#[test]
fn file_outline_not_found() {
    let (_repo_dir, db_dir, repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "nonexistent.rs",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn file_outline_fails_without_required_args() {
    let output = Command::new(codeatlas_bin())
        .args(["file-outline"])
        .output()
        .expect("file-outline");

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// file-tree command tests
// ---------------------------------------------------------------------------

#[test]
fn file_tree_lists_files() {
    let (_repo_dir, db_dir, repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-tree",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-tree");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-tree should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("entries:"));
    assert!(stdout.contains("src/lib.rs"));
    assert!(stdout.contains("src/main.rs"));
}

#[test]
fn file_tree_with_prefix_filter() {
    let (_repo_dir, db_dir, repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-tree",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
            "--prefix",
            "src/lib",
        ])
        .output()
        .expect("file-tree");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("src/lib.rs"));
    assert!(!stdout.contains("src/main.rs"));
}

#[test]
fn file_tree_fails_without_required_args() {
    let output = Command::new(codeatlas_bin())
        .args(["file-tree"])
        .output()
        .expect("file-tree");

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// repo-outline command tests
// ---------------------------------------------------------------------------

#[test]
fn repo_outline_shows_structure() {
    let (_repo_dir, db_dir, repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "repo-outline",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("repo-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "repo-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains(&format!("repo_id: {repo_id}")));
    assert!(stdout.contains("file_count:"));
    assert!(stdout.contains("symbol_count:"));
    assert!(stdout.contains("files:"));
}

#[test]
fn repo_outline_not_found() {
    let (_repo_dir, db_dir, _repo_id) = indexed_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "repo-outline",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            "nonexistent-repo",
        ])
        .output()
        .expect("repo-outline");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn repo_outline_fails_without_required_args() {
    let output = Command::new(codeatlas_bin())
        .args(["repo-outline"])
        .output()
        .expect("repo-outline");

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// mcp serve command tests
// ---------------------------------------------------------------------------

#[test]
fn mcp_serve_no_db_flag_uses_default_and_fails_when_missing() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve"])
        .output()
        .expect("mcp serve");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("database not found"),
        "should report database not found at default path, got: {stderr}"
    );
}

#[test]
fn mcp_serve_nonexistent_db_fails() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", "/nonexistent/path/index.db"])
        .output()
        .expect("mcp serve");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("database not found"),
        "should report missing database, got: {stderr}"
    );
}

#[test]
fn mcp_serve_valid_db_exits_zero_on_eof() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let _db = store::MetadataStore::open(&db_path).expect("open store");
    drop(_db);

    // .output() immediately closes stdin, so the server sees EOF and exits.
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("mcp serve");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "should exit 0 on clean EOF.\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("stdin closed"),
        "should log shutdown, got: {stderr}"
    );
}

#[test]
fn mcp_serve_stdout_is_empty() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let _db = store::MetadataStore::open(&db_path).expect("open store");
    drop(_db);

    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("mcp serve");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "stdout must be empty (reserved for protocol frames), got: {stdout}"
    );
}

#[test]
fn mcp_serve_help_exits_zero() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--help"])
        .output()
        .expect("mcp serve --help");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "help should exit 0.\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("Usage: codeatlas mcp serve"),
        "should print serve usage, got: {stderr}"
    );
}

#[test]
fn mcp_help_exits_zero() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "--help"])
        .output()
        .expect("mcp --help");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "mcp --help should exit 0.\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("Subcommands:"),
        "should list subcommands, got: {stderr}"
    );
}

#[test]
fn mcp_unknown_subcommand_fails() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "start"])
        .output()
        .expect("mcp start");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown mcp subcommand"),
        "should report unknown subcommand, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// mcp serve stdio protocol tests (subprocess)
// ---------------------------------------------------------------------------

use std::process::Stdio;

/// Send newline-delimited JSON messages to the MCP server subprocess and
/// collect stdout responses.
fn mcp_stdio_exchange(db_path: &std::path::Path, messages: &[&str]) -> (Vec<String>, String) {
    let mut child = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for msg in messages {
            writeln!(stdin, "{msg}").expect("write to stdin");
        }
    }
    // Drop stdin to close it, triggering EOF.
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let responses: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    (responses, stderr)
}

fn mcp_test_db() -> (TempDir, PathBuf) {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");
    let _db = store::MetadataStore::open(&db_path).expect("open store");
    (db_dir, db_path)
}

#[test]
fn mcp_stdio_initialize_handshake() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ],
    );

    assert_eq!(responses.len(), 1, "only initialize gets a response");
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(r["result"]["serverInfo"]["name"], "codeatlas");
}

#[test]
fn mcp_stdio_tools_list() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 10);

    // Every tool must have a description and a full inputSchema with
    // properties and required fields (issue #133).
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            tool["description"].is_string(),
            "{name} missing description"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name} schema type");
        assert!(
            schema["properties"].is_object(),
            "{name} missing properties"
        );
        assert!(schema["required"].is_array(), "{name} missing required");
    }
}

#[test]
fn mcp_stdio_tools_call() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_file_tree","arguments":{"repo_id":"nonexistent"}}}"#,
        ],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert!(r["result"]["content"].is_array());
}

#[test]
fn mcp_stdio_malformed_json_returns_error() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            "this is not json",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        ],
    );

    assert_eq!(responses.len(), 2);
    let err: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(err["error"]["code"], -32700);
    let ok: serde_json::Value = serde_json::from_str(&responses[1]).unwrap();
    assert!(ok["result"]["tools"].is_array());
}

#[test]
fn mcp_stdio_stdout_is_protocol_only() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        ],
    );

    // Every stdout line must be valid JSON.
    for line in &responses {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "stdout line is not valid JSON: {line}");
    }
}

#[test]
fn mcp_stdio_content_length_rejected() {
    let (_db_dir, db_path) = mcp_test_db();

    let mut child = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        // Send Content-Length framed input (wrong transport format).
        writeln!(stdin, "Content-Length: 47").expect("write");
        writeln!(stdin).expect("write blank");
        write!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize"}}"#).expect("write body");
        writeln!(stdin).expect("write newline");
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse response"))
        .collect();

    assert!(
        !responses.is_empty(),
        "should produce at least one error response"
    );
    assert_eq!(responses[0]["error"]["code"], -32700);
    let msg = responses[0]["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("Content-Length"),
        "error should mention Content-Length, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// mcp serve signal handling tests (subprocess)
// ---------------------------------------------------------------------------

/// Helper: send an initialize request, wait for the response on stdout to
/// confirm the server is running, then send a signal and close stdin.
/// Returns (exit status, stdout lines, stderr).
fn signal_test_helper(signal: libc::c_int) -> (std::process::ExitStatus, String, String) {
    use std::io::BufRead;
    use std::sync::mpsc;

    let (_db_dir, db_path) = mcp_test_db();

    let mut child = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    let pid = child.id();

    // Send an initialize request.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-11-25","capabilities":{{}},"clientInfo":{{"name":"test","version":"1"}}}}}}"#
        )
        .expect("write initialize");
        stdin.flush().expect("flush stdin");
    }

    // Read stdout in a separate thread. Use a channel to notify when the
    // first response line arrives — this confirms the server has fully
    // started (DB opened, signal handlers installed, serve loop entered).
    let stdout_handle = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    let reader_thread = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout_handle);
        let mut all_output = String::new();
        let mut notified = false;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    all_output.push_str(&line);
                    if !notified {
                        let _ = tx.send(());
                        notified = true;
                    }
                }
                Err(_) => break,
            }
        }
        all_output
    });

    // Block until the server has produced its first response, confirming
    // signal handlers are installed and the serve loop is running.
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("server should respond to initialize within 5s");

    // Now send the signal — the server is confirmed to be running.
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }

    // Close stdin so the blocking read_line returns EOF, allowing the serve
    // loop to reach the shutdown flag check.
    drop(child.stdin.take());

    let status = child.wait().expect("wait");
    let stdout = reader_thread.join().expect("reader thread");
    let stderr_handle = child.stderr.take();
    let stderr = stderr_handle
        .map(|mut h| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut h, &mut s).ok();
            s
        })
        .unwrap_or_default();

    (status, stdout, stderr)
}

#[test]
fn mcp_stdio_sigterm_clean_shutdown() {
    let (status, stdout, stderr) = signal_test_helper(libc::SIGTERM);

    assert!(
        status.success(),
        "should exit 0 on SIGTERM.\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("received signal") || stderr.contains("stdin closed"),
        "should log signal shutdown, got: {stderr}"
    );

    // Stdout must contain only valid JSON lines (no partial writes).
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(
            parsed.is_ok(),
            "stdout should only contain valid JSON, got: {line}"
        );
    }
}

#[test]
fn mcp_stdio_sigint_clean_shutdown() {
    let (status, stdout, stderr) = signal_test_helper(libc::SIGINT);

    assert!(
        status.success(),
        "should exit 0 on SIGINT.\nstderr: {stderr}"
    );

    // Stdout must contain only valid JSON lines.
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(
            parsed.is_ok(),
            "stdout should only contain valid JSON, got: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// mcp serve startup diagnostics (subprocess)
// ---------------------------------------------------------------------------

#[test]
fn mcp_serve_missing_db_diagnostic() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", "/nonexistent/path/to/index.db"])
        .output()
        .expect("run mcp serve");

    assert!(
        !output.status.success(),
        "should exit non-zero for missing DB"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("database not found"),
        "should report missing DB, got: {stderr}"
    );
    assert!(
        stderr.contains("codeatlas index"),
        "should hint about running index first, got: {stderr}"
    );

    // Stdout must be empty — no protocol output on startup failure.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be empty on startup failure, got: {stdout}"
    );
}

#[test]
fn mcp_serve_directory_as_db_diagnostic() {
    let dir = TempDir::new().expect("temp dir");

    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", dir.path().to_str().unwrap()])
        .output()
        .expect("run mcp serve");

    assert!(
        !output.status.success(),
        "should exit non-zero for directory path"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("directory"),
        "should report path is a directory, got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be empty on startup failure, got: {stdout}"
    );
}

#[test]
fn mcp_serve_unreadable_db_diagnostic() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("locked.db");
    std::fs::write(&db_path, "not a real database").expect("create file");

    // Remove read permission.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o000))
            .expect("set permissions");
    }

    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("run mcp serve");

    // Restore permissions so tempdir cleanup can succeed.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o644));
    }

    assert!(
        !output.status.success(),
        "should exit non-zero for unreadable DB"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not readable") || stderr.contains("cannot open"),
        "should report unreadable DB, got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be empty on startup failure, got: {stdout}"
    );
}

#[test]
fn mcp_serve_corrupt_db_diagnostic() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("corrupt.db");
    std::fs::write(&db_path, "this is not a sqlite database").expect("create file");

    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("run mcp serve");

    assert!(
        !output.status.success(),
        "should exit non-zero for corrupt DB"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to open database") || stderr.contains("not a database"),
        "should report open failure, got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be empty on startup failure, got: {stdout}"
    );
}

#[test]
fn mcp_serve_no_db_flag_default_path_diagnostic() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve"])
        .output()
        .expect("run mcp serve");

    assert!(
        !output.status.success(),
        "should exit non-zero when default db does not exist"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("database not found"),
        "should report database not found, got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be empty on startup failure, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// mcp serve full smoke test (subprocess)
// ---------------------------------------------------------------------------

#[test]
fn mcp_stdio_full_smoke_initialize_list_call() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            // 1. initialize
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke-test","version":"1.0"}}}"#,
            // 2. notifications/initialized
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            // 3. tools/list
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            // 4. tools/call — get_file_tree on empty DB
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_file_tree","arguments":{"repo_id":"test-repo"}}}"#,
        ],
    );

    // Should get exactly 3 responses (initialize, tools/list, tools/call).
    // notifications/initialized produces no response.
    assert_eq!(
        responses.len(),
        3,
        "expected 3 responses, got {}.\nstderr: {stderr}",
        responses.len()
    );

    // Verify initialize response.
    let r1: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(r1["id"], 1);
    assert_eq!(r1["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(r1["result"]["serverInfo"]["name"], "codeatlas");

    // Verify tools/list response.
    let r2: serde_json::Value = serde_json::from_str(&responses[1]).unwrap();
    assert_eq!(r2["id"], 2);
    let tools = r2["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 10);

    // Verify tools/call response.
    let r3: serde_json::Value = serde_json::from_str(&responses[2]).unwrap();
    assert_eq!(r3["id"], 3);
    assert!(
        r3["result"]["content"].is_array(),
        "tools/call should return content array"
    );

    // Every stdout line must be valid JSON (protocol-only assertion).
    for (i, line) in responses.iter().enumerate() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "response {i} is not valid JSON: {line}");
    }

    // Stderr should contain startup diagnostic, not be empty.
    assert!(
        stderr.contains("codeatlas mcp"),
        "stderr should contain server diagnostics, got: {stderr}"
    );
}

#[test]
fn mcp_stdio_invalid_tool_params_structured_error() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            // Call search_symbols without required 'query' field.
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_symbols","arguments":{"repo_id":"test"}}}"#,
        ],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();

    // Should be a successful JSON-RPC response wrapping an MCP error.
    assert!(
        r["result"].is_object(),
        "should be a result, not a JSON-RPC error"
    );
    assert_eq!(r["result"]["isError"], true);

    // The inner MCP response should contain a structured error.
    let content_text = r["result"]["content"][0]["text"].as_str().unwrap();
    let mcp_response: serde_json::Value = serde_json::from_str(content_text).unwrap();
    assert_eq!(mcp_response["status"], "error");
    assert!(
        mcp_response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("query"),
        "error should mention missing 'query' field"
    );
}

#[test]
fn mcp_stdio_unknown_tool_structured_error() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"does_not_exist","arguments":{}}}"#,
        ],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(r["result"]["isError"], true);

    let content_text = r["result"]["content"][0]["text"].as_str().unwrap();
    let mcp_response: serde_json::Value = serde_json::from_str(content_text).unwrap();
    assert_eq!(mcp_response["status"], "error");
    assert_eq!(mcp_response["error"]["code"], "unknown_tool");
}

#[test]
fn mcp_stdio_diagnostics_on_stderr_only() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ],
    );

    // Stderr should have diagnostics.
    assert!(
        !stderr.trim().is_empty(),
        "stderr should contain server diagnostics"
    );
    assert!(
        stderr.contains("codeatlas mcp"),
        "stderr should contain server identification"
    );

    // Stdout must contain only valid JSON-RPC responses.
    for line in &responses {
        let parsed: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|_| panic!("stdout line is not valid JSON: {line}"));
        assert!(
            parsed.get("jsonrpc").is_some(),
            "stdout line missing jsonrpc field: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// mcp serve client compatibility shims (subprocess)
// ---------------------------------------------------------------------------

#[test]
fn mcp_stdio_ping_returns_result() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) =
        mcp_stdio_exchange(&db_path, &[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#]);

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(r["id"], 1);
    assert!(r["result"].is_object());
    assert!(r.get("error").is_none(), "ping should not return an error");
}

#[test]
fn mcp_stdio_resources_list_returns_empty() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    let resources = r["result"]["resources"].as_array().unwrap();
    assert!(resources.is_empty());
}

#[test]
fn mcp_stdio_prompts_list_returns_empty() {
    let (_db_dir, db_path) = mcp_test_db();

    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"prompts/list"}"#],
    );

    assert_eq!(responses.len(), 1);
    let r: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    let prompts = r["result"]["prompts"].as_array().unwrap();
    assert!(prompts.is_empty());
}

#[test]
fn mcp_stdio_client_handshake_with_extra_capabilities() {
    let (_db_dir, db_path) = mcp_test_db();

    // Simulate a client that advertises capabilities the server doesn't use.
    let (responses, _stderr) = mcp_stdio_exchange(
        &db_path,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true},"sampling":{}},"clientInfo":{"name":"cursor","version":"0.50"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#,
            r#"{"jsonrpc":"2.0","id":5,"method":"prompts/list"}"#,
        ],
    );

    // 5 responses: initialize, ping, tools/list, resources/list, prompts/list.
    // notifications/initialized produces no response.
    assert_eq!(responses.len(), 5, "expected 5 responses");

    let r1: serde_json::Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(r1["result"]["protocolVersion"], "2025-11-25");

    let r2: serde_json::Value = serde_json::from_str(&responses[1]).unwrap();
    assert!(r2["result"].is_object(), "ping should return result");

    let r3: serde_json::Value = serde_json::from_str(&responses[2]).unwrap();
    assert_eq!(r3["result"]["tools"].as_array().unwrap().len(), 10);

    let r4: serde_json::Value = serde_json::from_str(&responses[3]).unwrap();
    assert!(r4["result"]["resources"].as_array().unwrap().is_empty());

    let r5: serde_json::Value = serde_json::from_str(&responses[4]).unwrap();
    assert!(r5["result"]["prompts"].as_array().unwrap().is_empty());

    // All stdout lines must be valid JSON-RPC.
    for line in &responses {
        let parsed: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("not valid JSON: {line}"));
        assert!(parsed.get("jsonrpc").is_some());
    }
}

// ---------------------------------------------------------------------------
// Help / unknown command
// ---------------------------------------------------------------------------

#[test]
fn help_flag_shows_usage() {
    let output = Command::new(codeatlas_bin())
        .args(["--help"])
        .output()
        .expect("help");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage:"));
}

#[test]
fn unknown_command_fails() {
    let output = Command::new(codeatlas_bin())
        .args(["nonexistent-command"])
        .output()
        .expect("unknown cmd");

    assert!(!output.status.success());
}

#[test]
fn no_args_shows_usage() {
    let output = Command::new(codeatlas_bin()).output().expect("no args");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage:"));
}

// ---------------------------------------------------------------------------
// Repo command tests
// ---------------------------------------------------------------------------

#[test]
fn repo_help_shows_subcommands() {
    let output = Command::new(codeatlas_bin())
        .args(["repo", "--help"])
        .output()
        .expect("repo help");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("add"));
    assert!(stderr.contains("list"));
    assert!(stderr.contains("status"));
    assert!(stderr.contains("refresh"));
    assert!(stderr.contains("remove"));
}

#[test]
fn repo_unknown_subcommand_fails() {
    let output = Command::new(codeatlas_bin())
        .args(["repo", "bogus"])
        .output()
        .expect("repo bogus");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown repo subcommand"));
}

#[test]
fn repo_add_list_status_refresh_remove_lifecycle() {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");

    // add
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "add",
            repo_dir.path().to_str().unwrap(),
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo add");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "repo add failed: {stdout}");
    assert!(stdout.contains("registered:"));

    // Derive the repo_id the same way the CLI does (directory name).
    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // list
    let output = Command::new(codeatlas_bin())
        .args(["repo", "list", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("repo list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "repo list failed: {stdout}");
    assert!(
        stdout.contains(&repo_id),
        "list should contain repo_id '{repo_id}': {stdout}"
    );

    // status
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "status",
            &repo_id,
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "repo status failed: {stdout}");
    assert!(stdout.contains("repo_id:"));
    assert!(stdout.contains("indexing_status:"));
    assert!(stdout.contains("ready"));

    // refresh
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "refresh",
            &repo_id,
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo refresh");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "repo refresh failed: {stdout}");
    assert!(stdout.contains("refreshed:"));

    // remove
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "remove",
            &repo_id,
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo remove");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "repo remove failed: {stdout}");
    assert!(stdout.contains("removed:"));

    // Verify repo is gone after remove.
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "status",
            &repo_id,
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo status after remove");
    assert!(!output.status.success());
}

#[test]
fn repo_list_empty_store() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");

    // Create an empty store by opening and closing it.
    store::MetadataStore::open(&db_path).expect("create empty store");

    let output = Command::new(codeatlas_bin())
        .args(["repo", "list", "--db", db_path.to_str().unwrap()])
        .output()
        .expect("repo list empty");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("No repositories registered"));
}

#[test]
fn repo_add_collision_different_source_root() {
    let repo_dir1 = setup_test_repo();
    let repo_dir2 = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");

    // Add first repo with explicit repo_id.
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "add",
            repo_dir1.path().to_str().unwrap(),
            "--repo-id",
            "shared-name",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("first repo add");
    assert!(output.status.success());

    // Add second repo with same repo_id but different path — should fail.
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "add",
            repo_dir2.path().to_str().unwrap(),
            "--repo-id",
            "shared-name",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("collision repo add");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already registered"));
}

#[test]
fn repo_remove_nonexistent() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");
    store::MetadataStore::open(&db_path).expect("create empty store");

    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "remove",
            "nonexistent",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("remove nonexistent");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn repo_remove_warns_on_blob_deletion_failure() {
    let repo_dir = setup_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");
    let blob_path = db_dir.path().join("blobs");

    // Add a repo so blobs get written.
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "add",
            repo_dir.path().to_str().unwrap(),
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo add");
    assert!(output.status.success());

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Collect the file hashes stored in the DB so we can sabotage a blob.
    let db = store::MetadataStore::open(&db_path).expect("open db");
    let hashes = db.files().list_hashes(&repo_id).expect("list hashes");
    drop(db);

    // Sabotage one blob by replacing the file with a directory, which makes
    // fs::remove_file fail with a "is a directory" error.
    if let Some(hash) = hashes.first() {
        let shard = &hash[..2];
        let blob_file = blob_path.join(shard).join(hash);
        if blob_file.exists() {
            std::fs::remove_file(&blob_file).expect("remove blob file");
            std::fs::create_dir(&blob_file).expect("replace blob with dir");
        }
    }

    // Remove the repo — should succeed but warn about the blob failure.
    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "remove",
            &repo_id,
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("repo remove with blob error");
    assert!(output.status.success(), "remove should still succeed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("warning: failed to delete blob"),
        "stderr should warn about blob failure: {stderr}"
    );
    assert!(
        stdout.contains("failed"),
        "stdout summary should mention failure count: {stdout}"
    );
}

#[test]
fn repo_status_nonexistent() {
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("metadata.db");
    store::MetadataStore::open(&db_path).expect("create empty store");

    let output = Command::new(codeatlas_bin())
        .args([
            "repo",
            "status",
            "nonexistent",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("status nonexistent");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

// ---------------------------------------------------------------------------
// Serve command tests
// ---------------------------------------------------------------------------

/// Starts `codeatlas serve` on port 0 and reads stderr with a timeout to
/// extract the actual listening address. Returns (child, addr_string).
/// Kills the child on timeout so tests don't hang.
fn start_serve(db_dir: &TempDir) -> (std::process::Child, String) {
    let mut child = std::process::Command::new(codeatlas_bin())
        .args([
            "serve",
            "--data-root",
            db_dir.path().to_str().unwrap(),
            "--port",
            "0",
        ])
        .env("CODEATLAS_LOG_FORMAT", "compact")
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn serve");

    let stderr = child.stderr.take().unwrap();

    // Read stderr in a background thread so we can apply a timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if let Some(start) = line.find("http://") {
                let addr = line[start..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("http://")
                    .to_string();
                let _ = tx.send(addr);
                return;
            }
        }
    });

    let addr = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap_or_else(|_| {
            let _ = child.kill();
            let _ = child.wait();
            panic!("service did not print listening address within 10 seconds");
        });

    (child, addr)
}

/// Send a minimal HTTP/1.1 request over raw TCP and return the response.
/// Avoids a dependency on curl or an HTTP client crate in tests.
fn http_get(addr: &str, path: &str) -> (u16, String) {
    let mut stream = std::net::TcpStream::connect(addr).expect("connect to service");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("send request");

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");

    // Parse status code from "HTTP/1.1 200 OK\r\n..."
    let status_code = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    // Extract body (after the blank line separating headers from body).
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_string();

    (status_code, body)
}

#[test]
fn serve_help_shows_usage() {
    let output = Command::new(codeatlas_bin())
        .args(["serve", "--help"])
        .output()
        .expect("serve help");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--data-root"));
    assert!(stderr.contains("--port"));
    assert!(stderr.contains("/health"));
}

#[test]
fn serve_starts_and_responds_to_health() {
    let db_dir = TempDir::new().expect("temp dir");
    let (mut child, addr) = start_serve(&db_dir);

    let (status, _body) = http_get(&addr, "/health");

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(status, 200, "health endpoint should return 200");
}

#[test]
fn serve_status_endpoint_returns_json() {
    let db_dir = TempDir::new().expect("temp dir");
    let (mut child, addr) = start_serve(&db_dir);

    let (status, body) = http_get(&addr, "/status");

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(status, 200);
    let json: serde_json::Value = serde_json::from_str(&body).expect("parse status JSON");
    assert_eq!(json["status"], "ok");
    assert!(json["uptime_secs"].is_number());
    assert!(json["repo_count"].is_number());
}

// ---------------------------------------------------------------------------
// MCP bridge tests
// ---------------------------------------------------------------------------

#[test]
fn mcp_bridge_help_shows_usage() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "bridge", "--help"])
        .output()
        .expect("bridge help");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--service-url"));
    assert!(stderr.contains("codeatlas mcp bridge"));
}

/// Helper: send MCP messages to a bridge subprocess and collect responses.
fn run_bridge_with_messages(service_addr: &str, mcp_messages: &[&str]) -> Vec<serde_json::Value> {
    let mut input = String::new();
    for msg in mcp_messages {
        input.push_str(msg);
        input.push('\n');
    }

    let output = Command::new(codeatlas_bin())
        .args(["mcp", "bridge", "--service-url", service_addr])
        .env("CODEATLAS_LOG_FORMAT", "compact")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).ok();
                // Close stdin to signal EOF so the bridge exits.
                drop(stdin);
            }
            child.wait_with_output()
        })
        .expect("run bridge");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse bridge response"))
        .collect()
}

#[test]
fn mcp_bridge_end_to_end_with_service() {
    let db_dir = TempDir::new().expect("temp dir");
    let (mut service_child, service_addr) = start_serve(&db_dir);

    let responses = run_bridge_with_messages(
        &service_addr,
        &[
            // initialize
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            // tools/list
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            // tools/call list_repos (proxied to service)
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_repos","arguments":{}}}"#,
        ],
    );

    let _ = service_child.kill();
    let _ = service_child.wait();

    assert_eq!(responses.len(), 3, "expected 3 responses: {responses:?}");

    // Initialize response.
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "codeatlas");

    // Tools list.
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.len() >= 10, "should list all tools");

    // Tools call (proxied through service).
    let content = responses[2]["result"]["content"]
        .as_array()
        .expect("content array");
    assert_eq!(content[0]["type"], "text");

    // Parse the inner MCP response envelope.
    let mcp_text = content[0]["text"].as_str().unwrap();
    let mcp_resp: serde_json::Value = serde_json::from_str(mcp_text).expect("parse MCP envelope");
    assert_eq!(mcp_resp["status"], "success");
}

#[test]
fn mcp_bridge_service_unreachable_fails_at_startup() {
    // Point bridge at a port nothing is listening on.
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "bridge", "--service-url", "127.0.0.1:1"])
        .env("CODEATLAS_LOG_FORMAT", "compact")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run bridge");

    assert!(
        !output.status.success(),
        "bridge should fail when service is unreachable"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot reach CodeAtlas service"),
        "stderr should explain the failure: {stderr}"
    );
}

#[test]
fn mcp_bridge_stdout_is_protocol_only() {
    let db_dir = TempDir::new().expect("temp dir");
    let (mut service_child, service_addr) = start_serve(&db_dir);

    let mut bridge_child = Command::new(codeatlas_bin())
        .args(["mcp", "bridge", "--service-url", &service_addr])
        .env("CODEATLAS_LOG_FORMAT", "compact")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bridge");

    // Send initialize then close stdin.
    if let Some(mut stdin) = bridge_child.stdin.take() {
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
            .ok();
        drop(stdin);
    }

    let output = bridge_child.wait_with_output().expect("bridge output");

    let _ = service_child.kill();
    let _ = service_child.wait();

    // Stdout should contain only valid JSON-RPC lines.
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        assert!(
            serde_json::from_str::<serde_json::Value>(line).is_ok(),
            "stdout line is not valid JSON: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// PHP / Laravel integration tests
// ---------------------------------------------------------------------------

fn setup_php_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");

    let controllers = dir.path().join("app/Http/Controllers");
    std::fs::create_dir_all(&controllers).expect("create controllers dir");
    std::fs::write(
        controllers.join("UserController.php"),
        r#"<?php

namespace App\Http\Controllers;

/**
 * Handles user-related HTTP requests.
 */
class UserController extends Controller
{
    public function index(): JsonResponse
    {
        return response()->json([]);
    }

    public function show(int $id): JsonResponse
    {
        return response()->json(null);
    }
}
"#,
    )
    .expect("write controller");

    let models = dir.path().join("app/Models");
    std::fs::create_dir_all(&models).expect("create models dir");
    std::fs::write(
        models.join("Post.php"),
        r#"<?php

namespace App\Models;

class Post
{
    const STATUS_DRAFT = 'draft';

    public function publish(): void
    {
    }
}
"#,
    )
    .expect("write model");

    dir
}

fn indexed_php_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_php_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&index_output.stdout);
    let stderr = String::from_utf8_lossy(&index_output.stderr);
    assert!(
        index_output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

#[test]
fn php_index_discovers_php_files_with_symbols() {
    let repo_dir = setup_php_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("files_discovered:"),
        "should report discovered files.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("symbols_extracted:"),
        "should report extracted symbols.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("files_with_symbols:"),
        "should report files with symbols.\nstdout: {stdout}"
    );
}

#[test]
fn php_file_outline_shows_controller_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_php_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "app/Http/Controllers/UserController.php",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserController"),
        "should show UserController in outline.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("index"),
        "should show index method in outline.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("show"),
        "should show show method in outline.\nstdout: {stdout}"
    );
}

#[test]
fn php_file_outline_shows_model_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_php_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "app/Models/Post.php",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Post"),
        "should show Post class.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("STATUS_DRAFT"),
        "should show class constant.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("publish"),
        "should show publish method.\nstdout: {stdout}"
    );
}

#[test]
fn php_search_symbols_finds_controller_class() {
    let (_repo_dir, db_dir, repo_id) = indexed_php_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "UserController",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserController"),
        "should find UserController.\nstdout: {stdout}"
    );
}

#[test]
fn php_search_symbols_finds_method() {
    let (_repo_dir, db_dir, repo_id) = indexed_php_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "publish",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("publish"),
        "should find publish method.\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Python integration tests
// ---------------------------------------------------------------------------

fn setup_python_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        dir.path().join("models.py"),
        r#"
class User:
    """Represents a user."""

    def __init__(self, name: str) -> None:
        self.name = name

    def get_name(self) -> str:
        """Return the user's name."""
        return self.name

class Admin(User):
    def promote(self) -> None:
        pass

def create_user(name: str) -> User:
    """Factory function."""
    return User(name)
"#,
    )
    .expect("write models.py");

    dir
}

fn indexed_python_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_python_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&index_output.stdout);
    let stderr = String::from_utf8_lossy(&index_output.stderr);
    assert!(
        index_output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

#[test]
fn python_index_discovers_files_with_symbols() {
    let repo_dir = setup_python_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("symbols_extracted:"));
}

#[test]
fn python_file_outline_shows_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_python_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "models.py",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("User"),
        "should show User class.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("get_name"),
        "should show get_name method.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("Admin"),
        "should show Admin class.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("create_user"),
        "should show create_user function.\nstdout: {stdout}"
    );
}

#[test]
fn python_search_symbols_finds_class() {
    let (_repo_dir, db_dir, repo_id) = indexed_python_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "User",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("User"),
        "should find User class.\nstdout: {stdout}"
    );
}

#[test]
fn python_search_symbols_finds_method() {
    let (_repo_dir, db_dir, repo_id) = indexed_python_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "get_name",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("get_name"),
        "should find get_name method.\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Go integration tests
// ---------------------------------------------------------------------------

fn setup_go_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        dir.path().join("main.go"),
        r#"package main

// Server holds HTTP configuration.
type Server struct {
    Addr string
}

// NewServer creates a new server.
func NewServer(addr string) *Server {
    return &Server{Addr: addr}
}

// Start starts the server.
func (s *Server) Start() error {
    return nil
}

const DefaultPort = 8080
"#,
    )
    .expect("write main.go");

    dir
}

fn indexed_go_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_go_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&index_output.stdout);
    let stderr = String::from_utf8_lossy(&index_output.stderr);
    assert!(
        index_output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

#[test]
fn go_index_discovers_files_with_symbols() {
    let repo_dir = setup_go_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("symbols_extracted:"));
}

#[test]
fn go_file_outline_shows_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_go_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "main.go",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Server"),
        "should show Server type.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("NewServer"),
        "should show NewServer function.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("Start"),
        "should show Start method.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("DefaultPort"),
        "should show DefaultPort constant.\nstdout: {stdout}"
    );
}

#[test]
fn go_search_symbols_finds_type() {
    let (_repo_dir, db_dir, repo_id) = indexed_go_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "Server",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Server"),
        "should find Server type.\nstdout: {stdout}"
    );
}

#[test]
fn go_search_symbols_finds_method() {
    let (_repo_dir, db_dir, repo_id) = indexed_go_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "Start",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Start"),
        "should find Start method.\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Java integration tests
// ---------------------------------------------------------------------------

fn setup_java_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let src = dir.path().join("src/main/java");
    std::fs::create_dir_all(&src).expect("create java dir");

    std::fs::write(
        src.join("UserController.java"),
        r#"
/**
 * Handles user HTTP requests.
 */
public class UserController {
    public List<User> index() {
        return null;
    }

    public User show(Long id) {
        return null;
    }
}

public enum Role {
    ADMIN,
    USER;
}
"#,
    )
    .expect("write java");

    dir
}

fn indexed_java_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_java_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&index_output.stdout);
    let stderr = String::from_utf8_lossy(&index_output.stderr);
    assert!(
        index_output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

#[test]
fn java_index_discovers_files_with_symbols() {
    let repo_dir = setup_java_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("symbols_extracted:"));
}

#[test]
fn java_file_outline_shows_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_java_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "src/main/java/UserController.java",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserController"),
        "should show UserController.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("index"),
        "should show index method.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("Role"),
        "should show Role enum.\nstdout: {stdout}"
    );
}

#[test]
fn java_search_symbols_finds_class() {
    let (_repo_dir, db_dir, repo_id) = indexed_java_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "UserController",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserController"),
        "should find UserController.\nstdout: {stdout}"
    );
}

#[test]
fn java_search_symbols_finds_method() {
    let (_repo_dir, db_dir, repo_id) = indexed_java_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "show",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("show"),
        "should find show method.\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// JavaScript integration tests
// ---------------------------------------------------------------------------

fn setup_js_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        dir.path().join("app.js"),
        r#"
/**
 * User service.
 */
class UserService {
    constructor(db) {
        this.db = db;
    }

    findAll() {
        return [];
    }

    findById(id) {
        return null;
    }
}

function createApp(config) {
    return { config };
}
"#,
    )
    .expect("write app.js");

    dir
}

fn indexed_js_test_repo() -> (TempDir, TempDir, String) {
    let repo_dir = setup_js_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let index_output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&index_output.stdout);
    let stderr = String::from_utf8_lossy(&index_output.stderr);
    assert!(
        index_output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let repo_id = repo_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    (repo_dir, db_dir, repo_id)
}

#[test]
fn js_index_discovers_files_with_symbols() {
    let repo_dir = setup_js_test_repo();
    let db_dir = TempDir::new().expect("db temp dir");
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args(["index", repo_dir.path().to_str().unwrap(), "--db"])
        .arg(&db_path)
        .output()
        .expect("index");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "index should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("symbols_extracted:"));
}

#[test]
fn js_file_outline_shows_symbols() {
    let (_repo_dir, db_dir, repo_id) = indexed_js_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "file-outline",
            "app.js",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("file-outline");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "file-outline should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserService"),
        "should show UserService.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("findAll"),
        "should show findAll method.\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("createApp"),
        "should show createApp function.\nstdout: {stdout}"
    );
}

#[test]
fn js_search_symbols_finds_class() {
    let (_repo_dir, db_dir, repo_id) = indexed_js_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "UserService",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("UserService"),
        "should find UserService.\nstdout: {stdout}"
    );
}

#[test]
fn js_search_symbols_finds_method() {
    let (_repo_dir, db_dir, repo_id) = indexed_js_test_repo();
    let db_path = db_dir.path().join("index.db");

    let output = Command::new(codeatlas_bin())
        .args([
            "search-symbols",
            "findAll",
            "--db",
            db_path.to_str().unwrap(),
            "--repo",
            &repo_id,
        ])
        .output()
        .expect("search-symbols");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("findAll"),
        "should find findAll method.\nstdout: {stdout}"
    );
}

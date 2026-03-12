//! Integration tests for the CLI commands.
//!
//! These tests exercise the CLI binary end-to-end by invoking it as a subprocess.

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
    assert!(stdout.contains("files_parsed:"));
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
fn mcp_serve_missing_db_fails() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve"])
        .output()
        .expect("mcp serve");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--db <path> is required"),
        "should report missing --db, got: {stderr}"
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

use std::io::Write;
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
    assert_eq!(tools.len(), 8);

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
fn mcp_serve_missing_db_flag_diagnostic() {
    let output = Command::new(codeatlas_bin())
        .args(["mcp", "serve"])
        .output()
        .expect("run mcp serve");

    assert!(
        !output.status.success(),
        "should exit non-zero for missing --db"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--db <path> is required"),
        "should report missing flag, got: {stderr}"
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
    assert_eq!(tools.len(), 8);

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
    assert_eq!(r3["result"]["tools"].as_array().unwrap().len(), 8);

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

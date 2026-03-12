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
        assert!(tool["description"].is_string(), "{name} missing description");
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name} schema type");
        assert!(
            schema["properties"].is_object(),
            "{name} missing properties"
        );
        assert!(
            schema["required"].is_array(),
            "{name} missing required"
        );
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

#[test]
fn mcp_stdio_sigterm_clean_shutdown() {
    let (_db_dir, db_path) = mcp_test_db();

    let mut child = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    let pid = child.id();

    // Send an initialize request so we know the server is running and responsive.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-11-25","capabilities":{{}},"clientInfo":{{"name":"test","version":"1"}}}}}}"#
        )
        .expect("write initialize");
    }

    // Give the server a moment to process, then send SIGTERM.
    std::thread::sleep(std::time::Duration::from_millis(100));
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }

    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "should exit 0 on SIGTERM.\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("received signal") || stderr.contains("stdin closed"),
        "should log signal shutdown, got: {stderr}"
    );

    // Stdout must contain only valid JSON lines (no partial writes).
    let stdout = String::from_utf8_lossy(&output.stdout);
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
    let (_db_dir, db_path) = mcp_test_db();

    let mut child = Command::new(codeatlas_bin())
        .args(["mcp", "serve", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp serve");

    let pid = child.id();

    // Send an initialize request so we know the server is running.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-11-25","capabilities":{{}},"clientInfo":{{"name":"test","version":"1"}}}}}}"#
        )
        .expect("write initialize");
    }

    std::thread::sleep(std::time::Duration::from_millis(100));
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGINT);
    }

    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "should exit 0 on SIGINT.\nstderr: {stderr}"
    );

    // Stdout must contain only valid JSON lines.
    let stdout = String::from_utf8_lossy(&output.stdout);
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

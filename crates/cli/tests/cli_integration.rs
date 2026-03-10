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

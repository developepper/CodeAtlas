//! End-to-end MCP integration tests.
//!
//! Indexes a real fixture repo via the indexer pipeline, then exercises
//! all MCP tools through the ToolRegistry backed by StoreQueryService.

use serde_json::json;
use tempfile::TempDir;

use core_model::{CapabilityTier, FileRecord, SymbolKind, SymbolRecord};
use indexer::{DefaultBackendRegistry, DispatchContext};
use query_engine::StoreQueryService;
use server_mcp::types::{ErrorCode, Status};
use server_mcp::ToolRegistry;
use syntax_platform::{PythonSyntaxBackend, RustSyntaxBackend};

// ── Test infrastructure ──────────────────────────────────────────────────

fn make_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry
}

/// Indexes a fixture repo and returns the store for querying.
fn indexed_store() -> (store::MetadataStore, store::BlobStore, TempDir, TempDir) {
    let (db, blob_store, blob_dir, repo_dir) = indexed_store_with_blobs();
    (db, blob_store, blob_dir, repo_dir)
}

/// Indexes a fixture repo and returns the store, blob store, blob dir, and repo dir
/// for tests that need blob-backed file content retrieval.
fn indexed_store_with_blobs() -> (store::MetadataStore, store::BlobStore, TempDir, TempDir) {
    let repo_dir = TempDir::new().expect("repo temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        concat!(
            "/// A greeting function.\n",
            "pub fn greet(name: &str) -> String {\n",
            "    format!(\"Hello, {name}!\")\n",
            "}\n",
            "\n",
            "pub struct Config {\n",
            "    pub verbose: bool,\n",
            "}\n",
            "\n",
            "impl Config {\n",
            "    pub fn new() -> Self {\n",
            "        Self { verbose: false }\n",
            "    }\n",
            "}\n",
        ),
    )
    .expect("write lib.rs");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let registry = make_registry();
    let ctx = indexer::PipelineContext {
        repo_id: "e2e-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store).expect("indexing should succeed");
    assert!(
        result.metrics.symbols_extracted > 0,
        "fixture repo should produce symbols"
    );

    (db, blob_store, blob_dir, repo_dir)
}

// ── search_symbols E2E ───────────────────────────────────────────────────

#[test]
fn e2e_search_symbols_finds_indexed_function() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "greet" }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty(), "should find greet symbol");
    assert_eq!(items[0]["name"], "greet");
    assert_eq!(items[0]["kind"], "function");
}

#[test]
fn e2e_search_symbols_no_match_returns_empty() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "zzz_no_match_zzz" }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(items.is_empty());
}

#[test]
fn e2e_search_symbols_empty_query_error() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "  " }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::InvalidParams);
}

#[test]
fn e2e_search_symbols_with_kind_filter() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "Config", "kind": "class" }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty());
    for item in &items {
        assert_eq!(item["kind"], "class");
    }
}

#[test]
fn e2e_search_symbols_with_limit() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "e", "limit": 1 }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    let items = payload["items"].as_array().unwrap();
    assert!(items.len() <= 1);
}

// ── get_symbol E2E ───────────────────────────────────────────────────────

#[test]
fn e2e_get_symbol_retrieves_by_id() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    // First search to get a real ID.
    let search = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "greet" }),
    );
    let id = search.payload.unwrap()["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = reg.call("get_symbol", json!({ "id": id }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["id"], id);
    assert_eq!(payload["name"], "greet");
}

#[test]
fn e2e_get_symbol_not_found() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_symbol", json!({ "id": "nonexistent-id" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── get_symbols E2E ──────────────────────────────────────────────────────

#[test]
fn e2e_get_symbols_batch_retrieval() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    // Search to get two real IDs.
    let search = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "e", "limit": 10 }),
    );
    let items = search.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(items.len() >= 2, "fixture should have at least 2 symbols");
    let ids: Vec<&str> = items
        .iter()
        .take(2)
        .map(|i| i["id"].as_str().unwrap())
        .collect();

    let resp = reg.call("get_symbols", json!({ "ids": ids }));
    assert_eq!(resp.status, Status::Success);
    let result_items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(result_items.len(), 2);
}

#[test]
fn e2e_get_symbols_skips_missing() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let search = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "greet" }),
    );
    let id = search.payload.unwrap()["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = reg.call("get_symbols", json!({ "ids": [id, "missing-id"] }));
    assert_eq!(resp.status, Status::Success);
    let result_items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(result_items.len(), 1);
}

// ── get_file_outline E2E ─────────────────────────────────────────────────

#[test]
fn e2e_get_file_outline_returns_symbols() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "e2e-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "src/lib.rs");
    let symbols = payload["symbols"].as_array().unwrap();
    assert!(
        symbols.len() >= 2,
        "lib.rs should have greet + Config + new"
    );
}

#[test]
fn e2e_get_file_outline_not_found() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "e2e-repo", "file_path": "nonexistent.rs" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── get_file_content E2E ─────────────────────────────────────────────────

#[test]
fn e2e_get_file_content_returns_source() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_store_with_blobs();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "e2e-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "src/lib.rs");
    let content = payload["content"].as_str().unwrap();
    assert!(
        !content.is_empty(),
        "content should not be empty when blob store is wired"
    );
    assert!(
        content.contains("pub fn greet"),
        "content should contain the actual source code"
    );
}

#[test]
fn e2e_get_file_content_not_found() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "e2e-repo", "file_path": "nonexistent.rs" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

#[test]
fn e2e_get_file_content_missing_blob_returns_error() {
    let (db, _blob_store, _blob_dir, _dir) = indexed_store();
    // Use a separate empty blob store — the file record exists but
    // the blob is missing, which is a data integrity error.
    let empty_blob_dir = TempDir::new().unwrap();
    let empty_blob_store =
        store::BlobStore::open(&empty_blob_dir.path().join("blobs")).expect("open blob store");
    let svc = StoreQueryService::new(&db, &empty_blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "e2e-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(
        resp.status,
        Status::Error,
        "missing blob should be an error"
    );
}

// ── get_file_tree E2E ────────────────────────────────────────────────────

#[test]
fn e2e_get_file_tree_returns_all_files() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "e2e-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2, "should have lib.rs and main.rs");
}

#[test]
fn e2e_get_file_tree_with_prefix_filter() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_tree",
        json!({ "repo_id": "e2e-repo", "path_prefix": "src/lib" }),
    );
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "src/lib.rs");
}

#[test]
fn e2e_get_file_tree_wrong_repo() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "unknown-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert!(entries.is_empty());
}

// ── get_repo_outline E2E ─────────────────────────────────────────────────

#[test]
fn e2e_get_repo_outline_returns_repo_and_files() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_repo_outline", json!({ "repo_id": "e2e-repo" }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["repo"]["repo_id"], "e2e-repo");
    assert!(payload["repo"]["file_count"].as_u64().unwrap() >= 2);
    assert!(payload["repo"]["symbol_count"].as_u64().unwrap() >= 2);
    assert!(!payload["files"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_get_repo_outline_not_found() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_repo_outline", json!({ "repo_id": "unknown-repo" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── search_text E2E ──────────────────────────────────────────────────────

#[test]
fn e2e_search_text_finds_content() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_text",
        json!({ "repo_id": "e2e-repo", "pattern": "greet" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    let items = payload["items"].as_array().unwrap();
    assert!(!items.is_empty(), "should find text matches for greet");
}

#[test]
fn e2e_search_text_empty_pattern_error() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_text",
        json!({ "repo_id": "e2e-repo", "pattern": "  " }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::InvalidParams);
}

// ── _meta envelope E2E ───────────────────────────────────────────────────

#[test]
fn e2e_meta_envelope_present_on_success() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "e2e-repo" }));
    assert_eq!(resp.status, Status::Success);
    assert!(!resp._meta.index_version.is_empty());
    let parts: Vec<&str> = resp._meta.index_version.split('.').collect();
    assert_eq!(parts.len(), 3, "index_version should be semver");
}

#[test]
fn e2e_meta_envelope_present_on_error() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_symbol", json!({ "id": "missing" }));
    assert_eq!(resp.status, Status::Error);
    assert!(!resp._meta.index_version.is_empty());
}

#[test]
fn e2e_meta_quality_stats_from_real_index() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "greet" }),
    );
    assert_eq!(resp.status, Status::Success);
    // Real indexed symbols via tree-sitter are Syntax quality.
    assert!((resp._meta.quality_stats.syntax_percent - 100.0).abs() < f64::EPSILON,);
}

// ── unknown tool E2E ─────────────────────────────────────────────────────

#[test]
fn e2e_unknown_tool_returns_error() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("nonexistent_tool", json!({}));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::UnknownTool);
}

// ── Structured error contract E2E ────────────────────────────────────────

#[test]
fn e2e_error_retryable_flag_is_false_for_invalid_params() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "  " }),
    );
    assert_eq!(resp.status, Status::Error);
    let err = resp.error.unwrap();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(!err.retryable);
}

#[test]
fn e2e_error_retryable_flag_is_false_for_not_found() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_symbol", json!({ "id": "missing" }));
    assert_eq!(resp.status, Status::Error);
    let err = resp.error.unwrap();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(!err.retryable);
}

// ── JSON round-trip E2E ──────────────────────────────────────────────────

#[test]
fn e2e_response_round_trips_through_json() {
    let (db, blob_store, _blob_dir, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "e2e-repo", "query": "greet" }),
    );
    let json_str = serde_json::to_string(&resp).expect("serialize");
    let deserialized: server_mcp::McpResponse =
        serde_json::from_str(&json_str).expect("deserialize");
    assert_eq!(resp, deserialized);
}

// ── File-only repo E2E (#167) ───────────────────────────────────────────

/// Indexes a repo with only file-only entries (no symbol adapters) and
/// returns stores for testing file-level query behavior.
fn indexed_file_only_store() -> (store::MetadataStore, store::BlobStore, TempDir, TempDir) {
    let repo_dir = TempDir::new().expect("repo temp dir");
    std::fs::write(repo_dir.path().join("app.py"), "print('hello')\n").expect("write py");
    std::fs::write(repo_dir.path().join("lib.go"), "package main\n").expect("write go");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let registry = make_registry(); // Only Rust backend — no Python/Go.
    let ctx = indexer::PipelineContext {
        repo_id: "file-only-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store).expect("indexing should succeed");
    assert_eq!(result.metrics.files_file_only, 2);
    assert_eq!(result.metrics.symbols_extracted, 0);

    (db, blob_store, blob_dir, repo_dir)
}

#[test]
fn e2e_file_only_repo_file_content_retrieval() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_file_only_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "file-only-repo", "file_path": "app.py" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "app.py");
    assert_eq!(payload["file"]["language"], "python");
    assert_eq!(payload["file"]["symbol_count"], 0);
    let content = payload["content"].as_str().unwrap();
    assert!(
        content.contains("print('hello')"),
        "should return actual file content for file-only indexed file"
    );
}

#[test]
fn e2e_file_only_repo_file_tree_includes_all_files() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_file_only_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "file-only-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(
        entries.len(),
        2,
        "both file-only files should appear in tree"
    );
}

#[test]
fn e2e_file_only_repo_file_outline_returns_empty_symbols() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_file_only_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "file-only-repo", "file_path": "app.py" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "app.py");
    let symbols = payload["symbols"].as_array().unwrap();
    assert!(symbols.is_empty(), "file-only files should have no symbols");
}

#[test]
fn e2e_file_only_repo_outline_reflects_file_only_entries() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_file_only_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_repo_outline", json!({ "repo_id": "file-only-repo" }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["repo"]["file_count"], 2);
    assert_eq!(payload["repo"]["symbol_count"], 0);
    let files = payload["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    for f in files {
        assert_eq!(f["symbol_count"], 0);
    }
}

#[test]
fn e2e_file_only_repo_unknown_path_not_found() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_file_only_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "file-only-repo", "file_path": "nonexistent.py" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── Multi-tier repo E2E (#182) ──────────────────────────────────────────

/// Indexes a repo with mixed tiers:
/// - `src/lib.rs` (Rust) → syntax-only with symbols (real indexer)
/// - `app/models.py` (Python) → syntax-only with symbols (real indexer)
/// - `README.md` (markdown) → file-only (real indexer, no syntax backend)
/// - `src/app.ts` (TypeScript) → syntax-plus-semantic (seeded after indexing)
fn indexed_multi_tier_store() -> (store::MetadataStore, store::BlobStore, TempDir, TempDir) {
    let repo_dir = TempDir::new().expect("repo temp dir");

    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "/// A function.\npub fn greet() {}\npub struct Config {}\n",
    )
    .expect("write lib.rs");

    let app = repo_dir.path().join("app");
    std::fs::create_dir_all(&app).expect("create app dir");
    std::fs::write(
        app.join("models.py"),
        "class User:\n    \"\"\"A user.\"\"\"\n    def get_name(self):\n        return self.name\n",
    )
    .expect("write models.py");

    std::fs::write(repo_dir.path().join("README.md"), "# Hello\n").expect("write readme");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry.register_syntax(
        PythonSyntaxBackend::backend_id(),
        Box::new(PythonSyntaxBackend::new()),
    );

    let ctx = indexer::PipelineContext {
        repo_id: "multi-tier-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store).expect("indexing should succeed");
    assert!(result.metrics.symbols_extracted > 0);
    assert!(result.metrics.files_file_only >= 1);

    // Seed a syntax-plus-semantic file (simulates a TypeScript file that was
    // enriched by a semantic backend). We cannot run a real semantic backend
    // in tests, so we directly insert the records at the expected tier.
    let ts_content = b"export function hello() {}\nexport class App {}\n";
    let ts_hash = store::content_hash(ts_content);
    blob_store.put(ts_content).unwrap();
    db.files()
        .upsert(&FileRecord {
            repo_id: "multi-tier-repo".into(),
            file_path: "src/app.ts".into(),
            language: "typescript".into(),
            file_hash: ts_hash,
            summary: "App source".into(),
            symbol_count: 2,
            capability_tier: CapabilityTier::SyntaxPlusSemantic,
            updated_at: "2026-03-16T00:00:00Z".into(),
        })
        .unwrap();
    for (name, kind) in [("hello", SymbolKind::Function), ("App", SymbolKind::Class)] {
        let qualified_name = format!("crate::{name}");
        db.symbols()
            .upsert(&SymbolRecord {
                id: core_model::build_symbol_id(
                    "multi-tier-repo",
                    "src/app.ts",
                    &qualified_name,
                    kind,
                )
                .unwrap(),
                repo_id: "multi-tier-repo".into(),
                file_path: "src/app.ts".into(),
                language: "typescript".into(),
                kind,
                name: name.into(),
                qualified_name,
                signature: format!("function {name}()"),
                start_line: 1,
                end_line: 2,
                start_byte: 0,
                byte_length: 25,
                content_hash: format!("hash-{name}"),
                capability_tier: CapabilityTier::SyntaxPlusSemantic,
                confidence_score: 0.95,
                source_backend: "semantic-typescript".into(),
                indexed_at: "2026-03-16T00:00:00Z".into(),
                docstring: None,
                summary: None,
                parent_symbol_id: None,
                keywords: None,
                decorators_or_attributes: None,
                semantic_refs: None,
                container_symbol_id: None,
                namespace_path: None,
                raw_kind: None,
                modifiers: None,
            })
            .unwrap();
    }
    // Note: we do NOT upsert the repo record here — INSERT OR REPLACE on
    // repos cascades deletes to files/symbols. The seeded TS file is visible
    // in queries without updating repo-level aggregates.

    (db, blob_store, blob_dir, repo_dir)
}

#[test]
fn e2e_multi_tier_file_outline_syntax_file_carries_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "multi-tier-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success, "error: {:?}", resp.error);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["capability_tier"], "syntax_only");
    let symbols = payload["symbols"].as_array().unwrap();
    assert!(!symbols.is_empty());
    assert_eq!(symbols[0]["capability_tier"], "syntax_only");
}

#[test]
fn e2e_multi_tier_file_outline_file_only_carries_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "multi-tier-repo", "file_path": "README.md" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["capability_tier"], "file_only");
    let symbols = payload["symbols"].as_array().unwrap();
    assert!(symbols.is_empty());
}

#[test]
fn e2e_multi_tier_file_outline_semantic_file_carries_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_outline",
        json!({ "repo_id": "multi-tier-repo", "file_path": "src/app.ts" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["capability_tier"], "syntax_plus_semantic");
    let symbols = payload["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0]["capability_tier"], "syntax_plus_semantic");
}

#[test]
fn e2e_multi_tier_file_content_carries_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "multi-tier-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["capability_tier"], "syntax_only");
    assert!(payload["content"].as_str().unwrap().contains("greet"));

    let resp2 = reg.call(
        "get_file_content",
        json!({ "repo_id": "multi-tier-repo", "file_path": "README.md" }),
    );
    assert_eq!(resp2.status, Status::Success);
    let payload2 = resp2.payload.unwrap();
    assert_eq!(payload2["file"]["capability_tier"], "file_only");
}

#[test]
fn e2e_multi_tier_file_tree_entries_carry_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "multi-tier-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert!(entries.len() >= 3);

    let readme = entries.iter().find(|e| e["path"] == "README.md").unwrap();
    assert_eq!(readme["capability_tier"], "file_only");
    assert_eq!(readme["symbol_count"], 0);

    let rs = entries.iter().find(|e| e["path"] == "src/lib.rs").unwrap();
    assert_eq!(rs["capability_tier"], "syntax_only");
    assert!(rs["symbol_count"].as_u64().unwrap() >= 2);

    let py = entries
        .iter()
        .find(|e| e["path"] == "app/models.py")
        .unwrap();
    assert_eq!(py["capability_tier"], "syntax_only");
    assert!(py["symbol_count"].as_u64().unwrap() >= 2);

    let ts = entries.iter().find(|e| e["path"] == "src/app.ts").unwrap();
    assert_eq!(ts["capability_tier"], "syntax_plus_semantic");
    assert_eq!(ts["symbol_count"], 2);
}

#[test]
fn e2e_multi_tier_repo_outline_files_carry_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_repo_outline", json!({ "repo_id": "multi-tier-repo" }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    let files = payload["files"].as_array().unwrap();
    assert!(files.len() >= 3);

    let tiers: Vec<&str> = files
        .iter()
        .filter_map(|f| f["capability_tier"].as_str())
        .collect();
    assert!(tiers.contains(&"file_only"));
    assert!(tiers.contains(&"syntax_only"));
    assert!(tiers.contains(&"syntax_plus_semantic"));
}

#[test]
fn e2e_multi_tier_search_symbols_finds_across_languages() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "search_symbols",
        json!({ "repo_id": "multi-tier-repo", "query": "greet User Config get_name" }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(
        items.len() >= 2,
        "should find symbols from multiple languages"
    );

    let languages: Vec<&str> = items
        .iter()
        .filter_map(|i| i["language"].as_str())
        .collect();
    assert!(languages.contains(&"rust"));
    assert!(languages.contains(&"python"));

    for item in &items {
        assert!(
            item["capability_tier"].is_string(),
            "each result should carry capability_tier"
        );
    }
}

#[test]
fn e2e_multi_tier_get_symbol_carries_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    let search = reg.call(
        "search_symbols",
        json!({ "repo_id": "multi-tier-repo", "query": "greet" }),
    );
    let items = search.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty());
    let id = items[0]["id"].as_str().unwrap();

    let resp = reg.call("get_symbol", json!({ "id": id }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["capability_tier"], "syntax_only");
    assert_eq!(payload["name"], "greet");
}

#[test]
fn e2e_multi_tier_search_symbols_filter_by_capability_tier() {
    let (db, blob_store, _blob_dir, _repo_dir) = indexed_multi_tier_store();
    let svc = StoreQueryService::new(&db, &blob_store);
    let reg = ToolRegistry::new(&svc);

    // Filter to syntax_plus_semantic only — should return TS symbols.
    let resp = reg.call(
        "search_symbols",
        json!({
            "repo_id": "multi-tier-repo",
            "query": "hello App greet Config User",
            "capability_tier": "syntax_plus_semantic"
        }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty(), "should find semantic symbols");
    for item in &items {
        assert_eq!(
            item["capability_tier"], "syntax_plus_semantic",
            "all results should be syntax_plus_semantic when filtered"
        );
    }

    // Filter to syntax_only — should exclude TS symbols.
    let resp2 = reg.call(
        "search_symbols",
        json!({
            "repo_id": "multi-tier-repo",
            "query": "hello App greet Config User",
            "capability_tier": "syntax_only"
        }),
    );
    assert_eq!(resp2.status, Status::Success);
    let items2 = resp2.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items2.is_empty());
    for item in &items2 {
        assert_eq!(item["capability_tier"], "syntax_only");
    }
}

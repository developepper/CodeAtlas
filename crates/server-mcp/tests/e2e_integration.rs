//! End-to-end MCP integration tests.
//!
//! Indexes a real fixture repo via the indexer pipeline, then exercises
//! all MCP tools through the ToolRegistry backed by StoreQueryService.

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use serde_json::json;
use tempfile::TempDir;

use query_engine::StoreQueryService;
use server_mcp::types::{ErrorCode, Status};
use server_mcp::ToolRegistry;

// ── Test infrastructure ──────────────────────────────────────────────────

struct TestRouter {
    adapters: Vec<TreeSitterAdapter>,
}

impl TestRouter {
    fn new() -> Self {
        let adapters = supported_languages()
            .iter()
            .filter_map(|lang| create_adapter(lang))
            .collect();
        Self { adapters }
    }
}

impl AdapterRouter for TestRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .filter(|a| a.language() == language)
            .map(|a| a as &dyn LanguageAdapter)
            .collect()
    }
}

/// Indexes a fixture repo and returns the store for querying.
fn indexed_store() -> (store::MetadataStore, TempDir) {
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

    let router = TestRouter::new();
    let ctx = indexer::PipelineContext {
        repo_id: "e2e-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
        use_git_diff: false,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store).expect("indexing should succeed");
    assert!(
        result.metrics.symbols_extracted > 0,
        "fixture repo should produce symbols"
    );

    (db, repo_dir)
}

// ── search_symbols E2E ───────────────────────────────────────────────────

#[test]
fn e2e_search_symbols_finds_indexed_function() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_symbol", json!({ "id": "nonexistent-id" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── get_symbols E2E ──────────────────────────────────────────────────────

#[test]
fn e2e_get_symbols_batch_retrieval() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "e2e-repo", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "src/lib.rs");
    assert!(payload["content"].is_string());
}

#[test]
fn e2e_get_file_content_not_found() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call(
        "get_file_content",
        json!({ "repo_id": "e2e-repo", "file_path": "nonexistent.rs" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── get_file_tree E2E ────────────────────────────────────────────────────

#[test]
fn e2e_get_file_tree_returns_all_files() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "e2e-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2, "should have lib.rs and main.rs");
}

#[test]
fn e2e_get_file_tree_with_prefix_filter() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "unknown-repo" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert!(entries.is_empty());
}

// ── get_repo_outline E2E ─────────────────────────────────────────────────

#[test]
fn e2e_get_repo_outline_returns_repo_and_files() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_repo_outline", json!({ "repo_id": "unknown-repo" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ── search_text E2E ──────────────────────────────────────────────────────

#[test]
fn e2e_search_text_finds_content() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_file_tree", json!({ "repo_id": "e2e-repo" }));
    assert_eq!(resp.status, Status::Success);
    assert!(!resp._meta.index_version.is_empty());
    let parts: Vec<&str> = resp._meta.index_version.split('.').collect();
    assert_eq!(parts.len(), 3, "index_version should be semver");
}

#[test]
fn e2e_meta_envelope_present_on_error() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("get_symbol", json!({ "id": "missing" }));
    assert_eq!(resp.status, Status::Error);
    assert!(!resp._meta.index_version.is_empty());
}

#[test]
fn e2e_meta_quality_stats_from_real_index() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
    let reg = ToolRegistry::new(&svc);

    let resp = reg.call("nonexistent_tool", json!({}));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::UnknownTool);
}

// ── Structured error contract E2E ────────────────────────────────────────

#[test]
fn e2e_error_retryable_flag_is_false_for_invalid_params() {
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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
    let (db, _dir) = indexed_store();
    let svc = StoreQueryService::new(&db);
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

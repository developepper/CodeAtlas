//! Integration tests for the MCP tool registry.
//!
//! Uses the in-memory StubQueryService from query-engine's test-support
//! feature to exercise all tool handlers end-to-end.

use query_engine::test_support::StubQueryService;
use serde_json::json;

use server_mcp::types::{ErrorCode, Status};
use server_mcp::{McpResponse, ToolRegistry};

fn call(tool: &str, params: serde_json::Value) -> McpResponse {
    let svc = StubQueryService::new();
    let reg = ToolRegistry::new(&svc);
    reg.call(tool, params)
}

// ---------------------------------------------------------------------------
// Registry basics
// ---------------------------------------------------------------------------

#[test]
fn registry_lists_all_eight_tools() {
    let svc = StubQueryService::new();
    let reg = ToolRegistry::new(&svc);
    let names = reg.tool_names();
    assert_eq!(names.len(), 8);
    assert!(names.contains(&"search_symbols"));
    assert!(names.contains(&"get_symbol"));
    assert!(names.contains(&"get_symbols"));
    assert!(names.contains(&"get_file_outline"));
    assert!(names.contains(&"get_file_content"));
    assert!(names.contains(&"get_file_tree"));
    assert!(names.contains(&"get_repo_outline"));
    assert!(names.contains(&"search_text"));
}

#[test]
fn unknown_tool_returns_error() {
    let resp = call("nonexistent_tool", json!({}));
    assert_eq!(resp.status, Status::Error);
    let err = resp.error.unwrap();
    assert_eq!(err.code, ErrorCode::UnknownTool);
    assert!(!err.retryable);
}

// ---------------------------------------------------------------------------
// _meta envelope contract
// ---------------------------------------------------------------------------

#[test]
fn success_response_carries_meta_with_timing() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    assert_eq!(resp.status, Status::Success);
    // timing_ms is populated (may be 0 for fast in-memory calls, but field exists)
    let _ = resp._meta.timing_ms;
    assert!(!resp._meta.index_version.is_empty());
}

#[test]
fn error_response_carries_meta() {
    let resp = call("get_symbol", json!({ "id": "missing" }));
    assert_eq!(resp.status, Status::Error);
    assert!(!resp._meta.index_version.is_empty());
}

#[test]
fn meta_truncated_reflects_payload_truncation() {
    // With limit=1 and 2 matches, truncated should be true.
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha", "limit": 1 }),
    );
    assert_eq!(resp.status, Status::Success);
    assert!(resp._meta.truncated);

    // With large limit, truncated should be false.
    let resp2 = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "beta", "limit": 100 }),
    );
    assert!(!resp2._meta.truncated);
}

#[test]
fn meta_quality_stats_defaults_to_syntax_for_non_symbol_tools() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    assert!((resp._meta.quality_stats.syntax_percent - 100.0).abs() < f64::EPSILON);
    assert!((resp._meta.quality_stats.semantic_percent - 0.0).abs() < f64::EPSILON);
}

#[test]
fn meta_quality_stats_computed_from_symbol_results() {
    // StubQueryService symbols all have QualityLevel::Syntax,
    // so search_symbols should report 100% syntax.
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha" }),
    );
    assert_eq!(resp.status, Status::Success);
    assert!((resp._meta.quality_stats.syntax_percent - 100.0).abs() < f64::EPSILON);
    assert!((resp._meta.quality_stats.semantic_percent - 0.0).abs() < f64::EPSILON);
}

#[test]
fn meta_quality_stats_computed_for_get_symbol() {
    let svc = StubQueryService::new();
    let id = svc.symbols[0].id.clone();
    let reg = ToolRegistry::new(&svc);
    let resp = reg.call("get_symbol", json!({ "id": id }));
    assert_eq!(resp.status, Status::Success);
    assert!((resp._meta.quality_stats.syntax_percent - 100.0).abs() < f64::EPSILON);
    assert!((resp._meta.quality_stats.semantic_percent - 0.0).abs() < f64::EPSILON);
}

#[test]
fn meta_index_version_is_semver() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    let parts: Vec<&str> = resp._meta.index_version.split('.').collect();
    assert_eq!(parts.len(), 3, "index_version should be semver");
}

#[test]
fn unknown_tool_error_also_carries_meta() {
    let resp = call("bad_tool", json!({}));
    assert_eq!(resp.status, Status::Error);
    assert!(!resp._meta.index_version.is_empty());
}

// ---------------------------------------------------------------------------
// search_symbols
// ---------------------------------------------------------------------------

#[test]
fn search_symbols_returns_matching_results() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    let items = payload["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(payload["total_candidates"].as_u64().unwrap() > 0);
}

#[test]
fn search_symbols_empty_query_returns_error() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "  " }),
    );
    assert_eq!(resp.status, Status::Error);
    let err = resp.error.unwrap();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(!err.retryable);
}

#[test]
fn search_symbols_invalid_params_returns_error() {
    let resp = call("search_symbols", json!({}));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::InvalidParams);
}

#[test]
fn search_symbols_with_kind_filter() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha", "kind": "type" }),
    );
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "type");
}

#[test]
fn search_symbols_invalid_kind_returns_error() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha", "kind": "widget" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::InvalidParams);
}

#[test]
fn search_symbols_carries_score() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "beta" }),
    );
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty());
    assert!(items[0]["score"].as_f64().unwrap() > 0.0);
}

// ---------------------------------------------------------------------------
// get_symbol
// ---------------------------------------------------------------------------

#[test]
fn get_symbol_returns_record() {
    let svc = StubQueryService::new();
    let id = svc.symbols[0].id.clone();
    let reg = ToolRegistry::new(&svc);
    let resp = reg.call("get_symbol", json!({ "id": id }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["id"], id);
    assert_eq!(payload["name"], "alpha");
}

#[test]
fn get_symbol_not_found() {
    let resp = call("get_symbol", json!({ "id": "nonexistent" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ---------------------------------------------------------------------------
// get_symbols
// ---------------------------------------------------------------------------

#[test]
fn get_symbols_returns_found_records() {
    let svc = StubQueryService::new();
    let ids: Vec<String> = svc.symbols.iter().take(2).map(|s| s.id.clone()).collect();
    let reg = ToolRegistry::new(&svc);
    let resp = reg.call("get_symbols", json!({ "ids": ids }));
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);
}

#[test]
fn get_symbols_skips_missing() {
    let svc = StubQueryService::new();
    let ids = vec![svc.symbols[0].id.clone(), "missing".to_string()];
    let reg = ToolRegistry::new(&svc);
    let resp = reg.call("get_symbols", json!({ "ids": ids }));
    assert_eq!(resp.status, Status::Success);
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
}

// ---------------------------------------------------------------------------
// get_file_outline
// ---------------------------------------------------------------------------

#[test]
fn get_file_outline_returns_file_and_symbols() {
    let resp = call(
        "get_file_outline",
        json!({ "repo_id": "repo-1", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "src/lib.rs");
    assert!(!payload["symbols"].as_array().unwrap().is_empty());
}

#[test]
fn get_file_outline_not_found() {
    let resp = call(
        "get_file_outline",
        json!({ "repo_id": "repo-1", "file_path": "nonexistent.rs" }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ---------------------------------------------------------------------------
// get_file_content
// ---------------------------------------------------------------------------

#[test]
fn get_file_content_returns_content() {
    let resp = call(
        "get_file_content",
        json!({ "repo_id": "repo-1", "file_path": "src/lib.rs" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["file"]["file_path"], "src/lib.rs");
    assert!(payload["content"].is_string());
}

// ---------------------------------------------------------------------------
// get_file_tree
// ---------------------------------------------------------------------------

#[test]
fn get_file_tree_returns_entries() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2);
}

#[test]
fn get_file_tree_with_prefix() {
    let resp = call(
        "get_file_tree",
        json!({ "repo_id": "repo-1", "path_prefix": "src/main" }),
    );
    assert_eq!(resp.status, Status::Success);
    let entries = resp.payload.unwrap()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "src/main.rs");
}

// ---------------------------------------------------------------------------
// get_repo_outline
// ---------------------------------------------------------------------------

#[test]
fn get_repo_outline_returns_repo_and_files() {
    let resp = call("get_repo_outline", json!({ "repo_id": "repo-1" }));
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert_eq!(payload["repo"]["repo_id"], "repo-1");
    assert!(!payload["files"].as_array().unwrap().is_empty());
}

#[test]
fn get_repo_outline_not_found() {
    let resp = call("get_repo_outline", json!({ "repo_id": "unknown" }));
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
}

// ---------------------------------------------------------------------------
// search_text
// ---------------------------------------------------------------------------

#[test]
fn search_text_empty_pattern_returns_error() {
    let resp = call(
        "search_text",
        json!({ "repo_id": "repo-1", "pattern": "  " }),
    );
    assert_eq!(resp.status, Status::Error);
    assert_eq!(resp.error.unwrap().code, ErrorCode::InvalidParams);
}

#[test]
fn search_text_returns_structured_result() {
    let resp = call(
        "search_text",
        json!({ "repo_id": "repo-1", "pattern": "hello" }),
    );
    assert_eq!(resp.status, Status::Success);
    let payload = resp.payload.unwrap();
    assert!(payload["items"].is_array());
    assert!(payload["total_candidates"].is_number());
    assert!(payload["truncated"].is_boolean());
}

// ---------------------------------------------------------------------------
// Response envelope structure
// ---------------------------------------------------------------------------

#[test]
fn success_response_has_no_error_field() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    assert_eq!(resp.status, Status::Success);
    assert!(resp.payload.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn error_response_has_no_payload_field() {
    let resp = call("get_symbol", json!({ "id": "missing" }));
    assert_eq!(resp.status, Status::Error);
    assert!(resp.payload.is_none());
    assert!(resp.error.is_some());
}

#[test]
fn response_round_trips_through_json() {
    let resp = call("get_file_tree", json!({ "repo_id": "repo-1" }));
    let json_str = serde_json::to_string(&resp).expect("serialize");
    let deserialized: McpResponse = serde_json::from_str(&json_str).expect("deserialize");
    assert_eq!(resp, deserialized);
}

#[test]
fn error_response_round_trips_through_json() {
    let resp = call("get_symbol", json!({ "id": "missing" }));
    let json_str = serde_json::to_string(&resp).expect("serialize");
    let deserialized: McpResponse = serde_json::from_str(&json_str).expect("deserialize");
    assert_eq!(resp, deserialized);
}

// ---------------------------------------------------------------------------
// Provenance: source_adapter in symbol payloads
// ---------------------------------------------------------------------------

#[test]
fn get_symbol_carries_source_adapter() {
    let svc = StubQueryService::new();
    let id = svc.symbols[0].id.clone();
    let reg = ToolRegistry::new(&svc);
    let resp = reg.call("get_symbol", json!({ "id": id }));
    let payload = resp.payload.unwrap();
    let adapter = payload["source_adapter"].as_str().unwrap();
    assert!(
        !adapter.is_empty(),
        "source_adapter should be non-empty, got: {adapter}"
    );
}

#[test]
fn search_symbols_results_carry_source_adapter() {
    let resp = call(
        "search_symbols",
        json!({ "repo_id": "repo-1", "query": "alpha" }),
    );
    let items = resp.payload.unwrap()["items"].as_array().unwrap().clone();
    assert!(!items.is_empty());
    for item in &items {
        assert!(
            item["source_adapter"].is_string(),
            "each search result should have source_adapter"
        );
    }
}

#[test]
fn get_file_outline_symbols_carry_source_adapter() {
    let resp = call(
        "get_file_outline",
        json!({ "repo_id": "repo-1", "file_path": "src/lib.rs" }),
    );
    let symbols = resp.payload.unwrap()["symbols"].as_array().unwrap().clone();
    for sym in &symbols {
        assert!(
            sym["source_adapter"].is_string(),
            "outline symbols should carry source_adapter"
        );
    }
}

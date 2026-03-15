//! Performance threshold tests for query-engine operations.
//!
//! These tests enforce the SLO targets from spec §13.4 and §15:
//! - `search_symbols` < 300ms (warmed index, p95)
//! - `get_symbol` < 120ms
//!
//! Thresholds are intentionally generous to accommodate CI runner variance.
//! Criterion benchmarks provide detailed profiling; these tests provide
//! hard-fail regression detection.

use std::fs;
use std::time::Instant;

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use indexer::{run, PipelineContext};
use query_engine::{
    FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService, RepoOutlineRequest,
    StoreQueryService, SymbolQuery, TextQuery,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

struct TreeSitterRouter {
    adapters: Vec<TreeSitterAdapter>,
}

impl TreeSitterRouter {
    fn new() -> Self {
        let adapters = supported_languages()
            .iter()
            .filter_map(|lang| create_adapter(lang))
            .collect();
        Self { adapters }
    }
}

impl AdapterRouter for TreeSitterRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .filter(|a| a.language() == language)
            .map(|a| a as &dyn LanguageAdapter)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Fixture setup
// ---------------------------------------------------------------------------

fn setup_populated_store(
    file_count: usize,
) -> (store::MetadataStore, store::BlobStore, TempDir, TempDir) {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    fs::create_dir_all(&src).expect("create src dir");

    for i in 0..file_count {
        let content = format!(
            "pub struct Component{i} {{}}\n\
             pub fn build_component_{i}() -> Component{i} {{ Component{i} {{}} }}\n\
             impl Component{i} {{\n    \
                 pub fn update(&self) -> u32 {{ {i} }}\n    \
                 pub fn draw(&self) -> String {{ format!(\"c-{i}\") }}\n\
             }}\n"
        );
        fs::write(src.join(format!("component_{i}.rs")), content).expect("write file");
    }
    fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "perf-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        policy_override: Some(AdapterPolicy::SyntaxOnly),
        correlation_id: None,
        use_git_diff: false,
    };

    run(&ctx, &mut db, &blob_store).expect("index should succeed");

    (db, blob_store, repo_dir, blob_dir)
}

/// Runs `op` `n` times, returning all durations sorted.
fn measure_latencies<F: FnMut()>(mut op: F, n: usize) -> Vec<std::time::Duration> {
    let mut durations = Vec::with_capacity(n);
    for _ in 0..n {
        let start = Instant::now();
        op();
        durations.push(start.elapsed());
    }
    durations.sort();
    durations
}

/// Returns the p95 value from a sorted duration slice.
fn p95(sorted: &[std::time::Duration]) -> std::time::Duration {
    let idx = (sorted.len() as f64 * 0.95) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// Threshold: search_symbols p95 < 300ms (spec §13.4)
// ---------------------------------------------------------------------------

#[test]
fn search_symbols_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    // Warm-up.
    for _ in 0..5 {
        let _ = svc.search_symbols(&SymbolQuery {
            repo_id: "perf-repo".to_string(),
            text: "Component".to_string(),
            limit: 10,
            offset: 0,
            filters: QueryFilters::default(),
        });
    }

    let latencies = measure_latencies(
        || {
            svc.search_symbols(&SymbolQuery {
                repo_id: "perf-repo".to_string(),
                text: "Component".to_string(),
                limit: 10,
                offset: 0,
                filters: QueryFilters::default(),
            })
            .expect("search should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(300);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "search_symbols p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: get_symbol p95 < 120ms (spec §13.4)
// ---------------------------------------------------------------------------

#[test]
fn get_symbol_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    // Find a real symbol ID.
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "perf-repo".to_string(),
            text: "Component0".to_string(),
            limit: 1,
            offset: 0,
            filters: QueryFilters::default(),
        })
        .expect("find symbol");
    let symbol_id = result.items[0].record.id.clone();

    // Warm-up.
    for _ in 0..5 {
        let _ = svc.get_symbol(&symbol_id);
    }

    let latencies = measure_latencies(
        || {
            svc.get_symbol(&symbol_id).expect("get should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(120);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "get_symbol p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: file_outline completes in reasonable time
// ---------------------------------------------------------------------------

#[test]
fn file_outline_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    let latencies = measure_latencies(
        || {
            svc.get_file_outline(&FileOutlineRequest {
                repo_id: "perf-repo".to_string(),
                file_path: "src/component_0.rs".to_string(),
            })
            .expect("outline should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(200);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "file_outline p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: file_tree completes in reasonable time
// ---------------------------------------------------------------------------

#[test]
fn file_tree_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    let latencies = measure_latencies(
        || {
            svc.get_file_tree(&FileTreeRequest {
                repo_id: "perf-repo".to_string(),
                path_prefix: None,
            })
            .expect("tree should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(200);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "file_tree p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: repo_outline completes in reasonable time
// ---------------------------------------------------------------------------

#[test]
fn repo_outline_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    let latencies = measure_latencies(
        || {
            svc.get_repo_outline(&RepoOutlineRequest {
                repo_id: "perf-repo".to_string(),
            })
            .expect("outline should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(300);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "repo_outline p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: search_text (FTS) completes in reasonable time
// ---------------------------------------------------------------------------

#[test]
fn search_text_p95_under_threshold() {
    let (db, blob_store, _repo, _blob) = setup_populated_store(50);
    let svc = StoreQueryService::new(&db, &blob_store);

    let latencies = measure_latencies(
        || {
            svc.search_text(&TextQuery {
                repo_id: "perf-repo".to_string(),
                pattern: "component".to_string(),
                filters: QueryFilters::default(),
                limit: 20,
                offset: 0,
            })
            .expect("search should succeed");
        },
        100,
    );

    let threshold = std::time::Duration::from_millis(300);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "search_text p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

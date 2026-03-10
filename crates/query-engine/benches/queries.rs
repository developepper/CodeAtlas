//! Criterion benchmarks for query-engine latency.
//!
//! Measures query p50/p95/p99 against a pre-populated in-memory store.
//! Covers the SLO targets from spec §13.4:
//! - `search_symbols` p95 < 300ms (warmed index)
//! - `get_symbol` p95 < 120ms
//!
//! Used for regression detection in CI (spec §13.1, §15).

use std::fs;

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use criterion::{criterion_group, criterion_main, Criterion};
use indexer::{run, PipelineContext};
use query_engine::StoreQueryService;
use query_engine::{
    FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService, RepoOutlineRequest,
    SymbolQuery, TextQuery,
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
// Fixture: pre-populated store
// ---------------------------------------------------------------------------

struct BenchFixture {
    db: store::MetadataStore,
    _repo_dir: TempDir,
    _blob_dir: TempDir,
}

fn create_populated_store(file_count: usize) -> BenchFixture {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    fs::create_dir_all(&src).expect("create src dir");

    for i in 0..file_count {
        let content = format!(
            "pub struct Widget{i} {{}}\n\
             pub fn create_widget_{i}() -> Widget{i} {{ Widget{i} {{}} }}\n\
             impl Widget{i} {{\n    \
                 pub fn process(&self) -> u32 {{ {i} }}\n    \
                 pub fn render(&self) -> String {{ format!(\"widget-{i}\") }}\n\
             }}\n"
        );
        fs::write(src.join(format!("widget_{i}.rs")), content).expect("write file");
    }
    fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "bench-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
        use_git_diff: false,
    };

    run(&ctx, &mut db, &blob_store).expect("index should succeed");

    BenchFixture {
        db,
        _repo_dir: repo_dir,
        _blob_dir: blob_dir,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_search_symbols(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_search_symbols");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    group.bench_function("exact_match", |b| {
        b.iter(|| {
            let query = SymbolQuery {
                repo_id: "bench-repo".to_string(),
                text: "Widget0".to_string(),
                limit: 10,
                offset: 0,
                filters: QueryFilters::default(),
            };
            let result = svc.search_symbols(&query).expect("search should succeed");
            assert!(!result.items.is_empty());
        });
    });

    group.bench_function("prefix_match", |b| {
        b.iter(|| {
            let query = SymbolQuery {
                repo_id: "bench-repo".to_string(),
                text: "create_widget".to_string(),
                limit: 20,
                offset: 0,
                filters: QueryFilters::default(),
            };
            svc.search_symbols(&query).expect("search should succeed");
        });
    });

    group.bench_function("no_match", |b| {
        b.iter(|| {
            let query = SymbolQuery {
                repo_id: "bench-repo".to_string(),
                text: "nonexistent_symbol_xyz".to_string(),
                limit: 10,
                offset: 0,
                filters: QueryFilters::default(),
            };
            let result = svc.search_symbols(&query).expect("search should succeed");
            assert!(result.items.is_empty());
        });
    });

    group.finish();
}

fn bench_get_symbol(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_get_symbol");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    // Find a real symbol ID to query.
    let query = SymbolQuery {
        repo_id: "bench-repo".to_string(),
        text: "Widget0".to_string(),
        limit: 1,
        offset: 0,
        filters: QueryFilters::default(),
    };
    let result = svc.search_symbols(&query).expect("find symbol");
    let symbol_id = result.items[0].record.id.clone();

    group.bench_function("by_id", |b| {
        b.iter(|| {
            svc.get_symbol(&symbol_id).expect("get should succeed");
        });
    });

    group.bench_function("not_found", |b| {
        b.iter(|| {
            let _ = svc.get_symbol("nonexistent-id-12345");
        });
    });

    group.finish();
}

fn bench_file_outline(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_file_outline");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    group.bench_function("single_file", |b| {
        b.iter(|| {
            let request = FileOutlineRequest {
                repo_id: "bench-repo".to_string(),
                file_path: "src/widget_0.rs".to_string(),
            };
            svc.get_file_outline(&request)
                .expect("outline should succeed");
        });
    });

    group.finish();
}

fn bench_file_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_file_tree");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    group.bench_function("full_tree", |b| {
        b.iter(|| {
            let request = FileTreeRequest {
                repo_id: "bench-repo".to_string(),
                path_prefix: None,
            };
            svc.get_file_tree(&request).expect("tree should succeed");
        });
    });

    group.finish();
}

fn bench_repo_outline(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_repo_outline");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    group.bench_function("full_outline", |b| {
        b.iter(|| {
            let request = RepoOutlineRequest {
                repo_id: "bench-repo".to_string(),
            };
            svc.get_repo_outline(&request)
                .expect("outline should succeed");
        });
    });

    group.finish();
}

fn bench_search_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_search_text");
    group.sample_size(50);

    let fixture = create_populated_store(50);
    let svc = StoreQueryService::new(&fixture.db);

    group.bench_function("fts_query", |b| {
        b.iter(|| {
            let query = TextQuery {
                repo_id: "bench-repo".to_string(),
                pattern: "widget".to_string(),
                filters: QueryFilters::default(),
                limit: 20,
                offset: 0,
            };
            svc.search_text(&query).expect("search should succeed");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_search_symbols,
    bench_get_symbol,
    bench_file_outline,
    bench_file_tree,
    bench_repo_outline,
    bench_search_text,
);
criterion_main!(benches);

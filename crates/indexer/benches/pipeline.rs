//! Criterion benchmarks for the indexing pipeline.
//!
//! Measures end-to-end pipeline throughput (discovery → parse → persist)
//! across synthetic repos. Used for regression detection in CI (spec §13.1, §15).

use std::fs;

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use indexer::{run, PipelineContext};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Router backed by real tree-sitter adapters
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

fn create_rust_repo(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let src = dir.path().join("src");
    fs::create_dir_all(&src).expect("create src dir");

    for i in 0..file_count {
        let content = format!(
            "pub struct Type{i} {{}}\n\
             pub fn func_{i}(x: &Type{i}) -> bool {{ true }}\n\
             impl Type{i} {{\n    \
                 pub fn method_{i}(&self) -> u32 {{ {i} }}\n\
             }}\n"
        );
        fs::write(src.join(format!("mod_{i}.rs")), content).expect("write file");
    }

    // Write a main.rs that references the modules.
    let main_content = (0..file_count)
        .map(|i| format!("mod mod_{i};"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\nfn main() {}\n";
    fs::write(src.join("main.rs"), main_content).expect("write main.rs");

    dir
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_pipeline_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_throughput");
    group.sample_size(10);

    let router = TreeSitterRouter::new();

    for &file_count in &[5, 20, 50] {
        let repo_dir = create_rust_repo(file_count);
        let blob_dir = TempDir::new().expect("blob temp dir");
        let blob_store =
            store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

        group.bench_with_input(
            BenchmarkId::new("end_to_end", file_count),
            &file_count,
            |b, _| {
                b.iter(|| {
                    // Fresh DB each iteration to measure full index (not incremental).
                    let mut db = store::MetadataStore::open_in_memory().expect("open store");

                    let ctx = PipelineContext {
                        repo_id: "bench-repo".to_string(),
                        source_root: repo_dir.path().to_path_buf(),
                        router: &router,
                        default_policy: AdapterPolicy::SyntaxOnly,
                        correlation_id: None,
                        use_git_diff: false,
                    };

                    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
                    assert!(result.metrics.files_discovered > 0);
                });
            },
        );
    }
    group.finish();
}

fn bench_incremental_reindex(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_reindex");
    group.sample_size(10);

    let router = TreeSitterRouter::new();
    let repo_dir = create_rust_repo(20);
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

    // Pre-populate the store with an initial index.
    let mut db = store::MetadataStore::open_in_memory().expect("open store");
    let ctx = PipelineContext {
        repo_id: "bench-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
        use_git_diff: false,
    };
    run(&ctx, &mut db, &blob_store).expect("initial index");

    group.bench_function("reindex_no_changes", |b| {
        b.iter(|| {
            let ctx = PipelineContext {
                repo_id: "bench-repo".to_string(),
                source_root: repo_dir.path().to_path_buf(),
                router: &router,
                default_policy: AdapterPolicy::SyntaxOnly,
                correlation_id: None,
                use_git_diff: false,
            };
            let result = run(&ctx, &mut db, &blob_store).expect("reindex should succeed");
            assert_eq!(
                result.metrics.files_unchanged,
                result.metrics.files_discovered
            );
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_pipeline_throughput,
    bench_incremental_reindex
);
criterion_main!(benches);

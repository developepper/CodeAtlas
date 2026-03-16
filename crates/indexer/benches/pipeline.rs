//! Criterion benchmarks for the indexing pipeline.

use std::fs;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use indexer::{run, DefaultBackendRegistry, DispatchContext, PipelineContext};
use syntax_platform::RustSyntaxBackend;
use tempfile::TempDir;

fn make_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry
}

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

    let main_content = (0..file_count)
        .map(|i| format!("mod mod_{i};"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\nfn main() {}\n";
    fs::write(src.join("main.rs"), main_content).expect("write main.rs");

    dir
}

fn bench_pipeline_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_throughput");
    group.sample_size(10);

    let registry = make_registry();

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
                    let mut db = store::MetadataStore::open_in_memory().expect("open store");

                    let ctx = PipelineContext {
                        repo_id: "bench-repo".to_string(),
                        source_root: repo_dir.path().to_path_buf(),
                        registry: &registry,
                        dispatch_context: DispatchContext::default(),
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

    let registry = make_registry();
    let repo_dir = create_rust_repo(20);
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

    let mut db = store::MetadataStore::open_in_memory().expect("open store");
    let ctx = PipelineContext {
        repo_id: "bench-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };
    run(&ctx, &mut db, &blob_store).expect("initial index");

    group.bench_function("reindex_no_changes", |b| {
        b.iter(|| {
            let ctx = PipelineContext {
                repo_id: "bench-repo".to_string(),
                source_root: repo_dir.path().to_path_buf(),
                registry: &registry,
                dispatch_context: DispatchContext::default(),
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

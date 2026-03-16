//! Performance threshold tests for the indexing pipeline.
//!
//! Enforces that key pipeline operations complete within defined time
//! budgets, providing hard-fail regression detection in CI (spec §13.1, §15).
//!
//! SLO reference (spec §13.4):
//! - Incremental index update visible < 10s on small repos.

use std::fs;
use std::time::Instant;

use indexer::{run, DefaultBackendRegistry, DispatchContext, PipelineContext};
use syntax_platform::RustSyntaxBackend;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Registry helper
// ---------------------------------------------------------------------------

fn make_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    let rust_backend = RustSyntaxBackend::new();
    let rust_id = RustSyntaxBackend::backend_id();
    registry.register_syntax(rust_id, Box::new(rust_backend));
    registry
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

fn create_rust_repo(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let src = dir.path().join("src");
    fs::create_dir_all(&src).expect("create src dir");

    for i in 0..file_count {
        let content = format!(
            "pub struct Model{i} {{}}\n\
             pub fn create_model_{i}() -> Model{i} {{ Model{i} {{}} }}\n\
             impl Model{i} {{\n    \
                 pub fn validate(&self) -> bool {{ true }}\n\
             }}\n"
        );
        fs::write(src.join(format!("model_{i}.rs")), content).expect("write file");
    }
    fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    dir
}

// ---------------------------------------------------------------------------
// Threshold: full pipeline on small repo < 10s (spec §13.4)
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_small_repo_under_threshold() {
    let registry = make_registry();
    let repo_dir = create_rust_repo(20);
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "perf-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let start = Instant::now();
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
    let elapsed = start.elapsed();

    let threshold = std::time::Duration::from_secs(10);
    assert!(
        elapsed < threshold,
        "full pipeline on 20-file repo took {elapsed:?}, exceeds threshold {threshold:?}"
    );
    assert!(result.metrics.files_discovered >= 20);
    assert!(result.metrics.symbols_extracted > 0);
}

// ---------------------------------------------------------------------------
// Threshold: incremental reindex (no changes) < 5s
// ---------------------------------------------------------------------------

#[test]
fn incremental_reindex_no_changes_under_threshold() {
    let registry = make_registry();
    let repo_dir = create_rust_repo(20);
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "perf-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    // Initial index.
    run(&ctx, &mut db, &blob_store).expect("initial index");

    // Re-index with no changes.
    let start = Instant::now();
    let result = run(&ctx, &mut db, &blob_store).expect("reindex should succeed");
    let elapsed = start.elapsed();

    let threshold = std::time::Duration::from_secs(5);
    assert!(
        elapsed < threshold,
        "incremental reindex took {elapsed:?}, exceeds threshold {threshold:?}"
    );
    // All files should be detected as unchanged.
    assert_eq!(
        result.metrics.files_unchanged, result.metrics.files_discovered,
        "all files should be unchanged on reindex"
    );
}

// ---------------------------------------------------------------------------
// Threshold: pipeline throughput on larger repo
// ---------------------------------------------------------------------------

#[test]
fn pipeline_50_files_under_threshold() {
    let registry = make_registry();
    let repo_dir = create_rust_repo(50);
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store =
        store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "perf-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let start = Instant::now();
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
    let elapsed = start.elapsed();

    let threshold = std::time::Duration::from_secs(30);
    assert!(
        elapsed < threshold,
        "pipeline on 50-file repo took {elapsed:?}, exceeds threshold {threshold:?}"
    );
    assert!(result.metrics.files_discovered >= 50);
}

//! Criterion benchmarks for repository discovery throughput.
//!
//! Measures wall-clock time for `walk_repository` across synthetic repos
//! of varying sizes. Used for regression detection in CI (spec §13.1, §15).

use std::fs;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use repo_walker::{walk_repository, WalkerOptions};
use tempfile::TempDir;

/// Creates a temporary repo with `n` Rust source files spread across directories.
fn create_fixture_repo(n: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    for i in 0..n {
        let subdir = format!("src/pkg{}", i / 10);
        let path = dir.path().join(&subdir);
        fs::create_dir_all(&path).expect("create dir");
        fs::write(
            path.join(format!("file_{i}.rs")),
            format!("pub fn func_{i}() {{}}\n"),
        )
        .expect("write file");
    }
    dir
}

fn bench_discovery_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_throughput");
    group.sample_size(20);

    for &file_count in &[50, 100, 500] {
        let repo = create_fixture_repo(file_count);
        let options = WalkerOptions::default();

        group.bench_with_input(BenchmarkId::new("walk", file_count), &file_count, |b, _| {
            b.iter(|| {
                let result = walk_repository(repo.path(), &options).expect("walk should succeed");
                assert_eq!(result.metrics.files_discovered, file_count);
            });
        });
    }
    group.finish();
}

fn bench_discovery_with_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_with_filters");
    group.sample_size(20);

    let repo = create_fixture_repo(200);

    // Baseline: no extra filters.
    group.bench_function("no_filters", |b| {
        let options = WalkerOptions::default();
        b.iter(|| walk_repository(repo.path(), &options).expect("walk"));
    });

    // With extra ignore rules.
    group.bench_function("with_ignore_rules", |b| {
        let options = WalkerOptions {
            extra_ignore_rules: vec!["src/pkg0/**".to_string(), "src/pkg1/**".to_string()],
            ..WalkerOptions::default()
        };
        b.iter(|| walk_repository(repo.path(), &options).expect("walk"));
    });

    // With file size cap.
    group.bench_function("with_size_cap", |b| {
        let options = WalkerOptions {
            max_file_size_bytes: Some(1024),
            ..WalkerOptions::default()
        };
        b.iter(|| walk_repository(repo.path(), &options).expect("walk"));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_discovery_throughput,
    bench_discovery_with_filters
);
criterion_main!(benches);

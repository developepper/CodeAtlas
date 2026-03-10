//! Performance threshold tests for repository discovery.
//!
//! Enforces that discovery operations complete within defined time budgets,
//! providing hard-fail regression detection in CI (spec §13.1, §15).

use std::fs;
use std::time::Instant;

use repo_walker::{walk_repository, WalkerOptions};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

fn create_fixture_repo(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    for i in 0..file_count {
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
// Threshold: discovery of 100 files p95 < 500ms
// ---------------------------------------------------------------------------

#[test]
fn discovery_100_files_p95_under_threshold() {
    let repo = create_fixture_repo(100);
    let options = WalkerOptions::default();

    // Warm-up.
    for _ in 0..3 {
        let _ = walk_repository(repo.path(), &options);
    }

    let latencies = measure_latencies(
        || {
            let result = walk_repository(repo.path(), &options).expect("walk should succeed");
            assert_eq!(result.metrics.files_discovered, 100);
        },
        50,
    );

    let threshold = std::time::Duration::from_millis(500);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "discovery 100 files p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: discovery of 500 files < 3s
// ---------------------------------------------------------------------------

#[test]
fn discovery_500_files_under_threshold() {
    let repo = create_fixture_repo(500);
    let options = WalkerOptions::default();

    let start = Instant::now();
    let result = walk_repository(repo.path(), &options).expect("walk should succeed");
    let elapsed = start.elapsed();

    assert_eq!(result.metrics.files_discovered, 500);

    let threshold = std::time::Duration::from_secs(3);
    assert!(
        elapsed < threshold,
        "discovery 500 files took {elapsed:?}, exceeds threshold {threshold:?}"
    );
}

// ---------------------------------------------------------------------------
// Threshold: discovery with ignore rules p95 < 500ms
// ---------------------------------------------------------------------------

#[test]
fn discovery_with_filters_p95_under_threshold() {
    let repo = create_fixture_repo(200);
    let options = WalkerOptions {
        extra_ignore_rules: vec!["src/pkg0/**".to_string(), "src/pkg1/**".to_string()],
        max_file_size_bytes: Some(1024),
        ..WalkerOptions::default()
    };

    let latencies = measure_latencies(
        || {
            walk_repository(repo.path(), &options).expect("walk should succeed");
        },
        50,
    );

    let threshold = std::time::Duration::from_millis(500);
    let measured = p95(&latencies);
    assert!(
        measured < threshold,
        "discovery with filters p95 = {measured:?}, exceeds threshold {threshold:?}"
    );
}

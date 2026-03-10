//! Security regression tests for the repository walker.
//!
//! Covers:
//! - Symlink escape attempts (§11.2, §16.1).
//! - Resource exhaustion via file-count and file-size limits.
//! - Malformed filesystem entries (empty files, deeply nested paths).

use repo_walker::{walk_repository, WalkerOptions};

mod common;
use common::FixtureRepo;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn relative_paths(results: &[repo_walker::DiscoveredFile]) -> Vec<String> {
    results
        .iter()
        .map(|item| item.relative_path.to_string_lossy().replace('\\', "/"))
        .collect()
}

// ---------------------------------------------------------------------------
// Symlink escape tests
// ---------------------------------------------------------------------------

/// A symlink pointing to a file outside the repo root must be skipped.
#[test]
fn symlink_to_absolute_path_outside_repo_is_skipped() {
    let outside = FixtureRepo::new().expect("create outside fixture");
    outside
        .write("secret.txt", "top-secret-data")
        .expect("write secret");

    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("src/main.rs", "fn main() {}\n")
        .expect("write source");

    // Create symlink pointing to the file in the other temp dir.
    let link_path = repo.path().join("src/secret-link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), &link_path)
        .expect("create symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(outside.path().join("secret.txt"), &link_path)
        .expect("create symlink");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert!(
        !paths.iter().any(|p| p.contains("secret-link")),
        "symlink to outside repo should be skipped"
    );
    assert!(
        result.metrics.files_skipped_symlink >= 1,
        "should count at least one skipped symlink"
    );
}

/// A symlink pointing to the parent directory (traversal escape) must be
/// skipped and must not cause the walker to recurse outside the root.
#[cfg(unix)]
#[test]
fn symlink_to_parent_directory_is_skipped() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("src/main.rs", "fn main() {}\n")
        .expect("write source");

    // Create a directory symlink pointing to ".." (parent).
    let link_path = repo.path().join("escape");
    std::os::unix::fs::symlink("..", &link_path).expect("create parent symlink");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    // The walker must not traverse through the symlink.
    assert!(
        !paths.iter().any(|p| p.starts_with("escape/")),
        "walker must not follow directory symlink to parent: {paths:?}"
    );
}

/// A deeply nested chain of symlinks that eventually escapes the repo
/// must be skipped safely.
#[cfg(unix)]
#[test]
fn deeply_nested_symlink_chain_is_skipped() {
    let outside = FixtureRepo::new().expect("create outside fixture");
    outside
        .write("passwd", "root:x:0:0:root:/root")
        .expect("write outside file");

    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("src/main.rs", "fn main() {}\n")
        .expect("write source");

    // Create a chain: link-a → link-b → outside/passwd
    let link_b = repo.path().join("src/link-b");
    std::os::unix::fs::symlink(outside.path().join("passwd"), &link_b).expect("create link-b");

    let link_a = repo.path().join("src/link-a");
    std::os::unix::fs::symlink(&link_b, &link_a).expect("create link-a");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert!(
        !paths.iter().any(|p| p.contains("link-a")),
        "symlink chain should not be followed: {paths:?}"
    );
    assert!(
        result.metrics.files_skipped_symlink >= 1,
        "should count skipped symlinks"
    );
}

// ---------------------------------------------------------------------------
// Resource exhaustion tests
// ---------------------------------------------------------------------------

/// Exceeding the file-count limit returns an error, not a panic.
#[test]
fn file_count_limit_returns_error_not_panic() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    for i in 0..10 {
        repo.write(&format!("file_{i}.txt"), &format!("content {i}"))
            .expect("write file");
    }

    let options = WalkerOptions {
        max_file_count: Some(5),
        ..WalkerOptions::default()
    };
    let err = walk_repository(repo.path(), &options).expect_err("should hit file count limit");
    let msg = err.to_string();
    assert!(
        msg.contains("file_count") && msg.contains("5"),
        "error should mention file_count limit: {msg}"
    );
}

/// The file-size cap prevents large files from being included, and the
/// walker completes successfully (does not OOM or panic).
#[test]
fn file_size_cap_prevents_large_file_inclusion() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("small.rs", "fn small() {}\n")
        .expect("write small");

    // Create a file just over the limit.
    let large_content = "x".repeat(1_000_001);
    repo.write("large.txt", &large_content)
        .expect("write large");

    let options = WalkerOptions {
        max_file_size_bytes: Some(1_000_000),
        ..WalkerOptions::default()
    };
    let result = walk_repository(repo.path(), &options).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert!(
        !paths.contains(&"large.txt".to_string()),
        "large file should be excluded"
    );
    assert_eq!(result.metrics.files_skipped_size, 1);
}

/// Many small files close to the count limit: the walker must either
/// succeed or return a clean LimitExceeded error.
#[test]
fn many_files_near_limit_boundary() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    let count = 100;
    for i in 0..count {
        repo.write(&format!("dir/f{i:04}.txt"), "x")
            .expect("write file");
    }

    // Limit is exactly the count — should succeed.
    let options = WalkerOptions {
        max_file_count: Some(count),
        ..WalkerOptions::default()
    };
    let result = walk_repository(repo.path(), &options).expect("walk should succeed at limit");
    assert_eq!(result.metrics.files_discovered, count);

    // Limit is one less — should fail.
    let options = WalkerOptions {
        max_file_count: Some(count - 1),
        ..WalkerOptions::default()
    };
    let err = walk_repository(repo.path(), &options).expect_err("should exceed limit by one file");
    assert!(err.to_string().contains("file_count"));
}

// ---------------------------------------------------------------------------
// Malformed filesystem entry tests
// ---------------------------------------------------------------------------

/// Empty files should be discovered and not cause panics.
#[test]
fn empty_files_are_discovered_safely() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("empty.rs", "").expect("write empty");
    repo.write("also-empty.py", "").expect("write empty");
    repo.write("has-content.rs", "fn main() {}\n")
        .expect("write content");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");

    // Empty files should still be discovered (they're valid text files).
    assert!(
        result.metrics.files_discovered >= 3,
        "empty files should be counted: got {}",
        result.metrics.files_discovered
    );
}

/// Files with null bytes should be classified as binary and skipped.
#[test]
fn files_with_null_bytes_are_skipped_as_binary() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("clean.rs", "fn main() {}\n")
        .expect("write clean");
    repo.write_bytes("nulls.txt", b"hello\x00world")
        .expect("write file with null");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert!(
        !paths.contains(&"nulls.txt".to_string()),
        "file with null bytes should be skipped as binary"
    );
    assert_eq!(result.metrics.files_skipped_binary, 1);
}

/// Deeply nested directory structures should be traversed without stack
/// overflow or other failures.
#[test]
fn deeply_nested_directories_are_traversed() {
    let repo = FixtureRepo::new().expect("create repo fixture");

    // Create a path 50 levels deep.
    let deep_path = (0..50).fold(String::new(), |mut acc, i| {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(&format!("d{i}"));
        acc
    });
    let file_path = format!("{deep_path}/deep.rs");
    repo.write(&file_path, "fn deep() {}\n")
        .expect("write deep file");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert!(
        paths.iter().any(|p| p.contains("deep.rs")),
        "deeply nested file should be discovered: {paths:?}"
    );
}

/// Filenames with special characters should not cause panics.
#[test]
fn filenames_with_special_characters() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("normal.rs", "fn normal() {}\n")
        .expect("write normal");
    repo.write("spaces in name.txt", "content")
        .expect("write spaced");
    repo.write("special-chars_v2.0.txt", "content")
        .expect("write special");
    repo.write("unicode_日本語.txt", "content")
        .expect("write unicode");

    let result = walk_repository(repo.path(), &WalkerOptions::default()).expect("walk repo");
    assert!(
        result.metrics.files_discovered >= 4,
        "all files with special names should be discovered: got {}",
        result.metrics.files_discovered
    );
}

/// A file that is a symlink to itself (circular) must not hang the walker.
#[cfg(unix)]
#[test]
fn circular_symlink_does_not_hang() {
    let repo = FixtureRepo::new().expect("create repo fixture");
    repo.write("real.rs", "fn real() {}\n").expect("write real");

    // Create a self-referencing symlink.
    let link = repo.path().join("loop.txt");
    // On some systems creating a symlink to itself may fail — that's fine,
    // the test is still valid if the walker doesn't hang.
    let _ = std::os::unix::fs::symlink(&link, &link);

    // The key assertion: this must complete (not hang or panic).
    let result = walk_repository(repo.path(), &WalkerOptions::default());
    // Either succeeds with the real file, or returns an error — both OK.
    match result {
        Ok(r) => {
            let paths = relative_paths(&r.files);
            assert!(
                paths.contains(&"real.rs".to_string()),
                "real file should still be discovered"
            );
        }
        Err(e) => {
            // An error is acceptable — hanging is not.
            let _ = e;
        }
    }
}

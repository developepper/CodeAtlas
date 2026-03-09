use std::path::Path;

use repo_walker::{walk_repository, WalkerOptions};

mod common;
use common::FixtureRepo;

#[test]
fn walker_honors_gitignore_and_returns_deterministic_order() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write(
            ".gitignore",
            "target/\nlogs/*.log\nsubdir/*.tmp\n!subdir/keep.tmp\n",
        )
        .expect("write .gitignore");
    fixture.write("README.md", "# Repo\n").expect("write file");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write file");
    fixture
        .write("target/out.txt", "ignored\n")
        .expect("write file");
    fixture
        .write("logs/run.log", "ignored\n")
        .expect("write file");
    fixture
        .write("subdir/skip.tmp", "ignored\n")
        .expect("write file");
    fixture
        .write("subdir/keep.tmp", "kept\n")
        .expect("write file");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert_eq!(
        paths,
        vec![
            ".gitignore".to_string(),
            "README.md".to_string(),
            "src/main.rs".to_string(),
            "subdir/keep.tmp".to_string(),
        ]
    );
    assert_eq!(result.metrics.files_discovered, 4);
}

#[test]
fn walker_applies_extra_ignore_rules_with_negation() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture.write("README.md", "# Repo\n").expect("write file");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write file");
    fixture
        .write("src/lib.rs", "pub fn lib() {}\n")
        .expect("write file");

    let options = WalkerOptions {
        extra_ignore_rules: vec!["src/**".to_string(), "!src/main.rs".to_string()],
        ..WalkerOptions::default()
    };
    let result = walk_repository(fixture.path(), &options).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert_eq!(
        paths,
        vec!["README.md".to_string(), "src/main.rs".to_string()]
    );
    assert_eq!(result.metrics.files_discovered, 2);
    assert_eq!(result.metrics.files_skipped_extra_rules, 1);
}

#[test]
fn walker_honors_dot_ignore_files() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write(".ignore", "tmp/**\n!tmp/keep.txt\n")
        .expect("write .ignore");
    fixture
        .write("tmp/skip.txt", "ignored\n")
        .expect("write file");
    fixture.write("tmp/keep.txt", "keep\n").expect("write file");
    fixture.write("README.md", "# Repo\n").expect("write file");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);

    assert_eq!(
        paths,
        vec![
            ".ignore".to_string(),
            "README.md".to_string(),
            "tmp/keep.txt".to_string(),
        ]
    );
}

#[test]
fn walker_rejects_invalid_root() {
    let missing = Path::new("/definitely/not/a/repo");
    let err = walk_repository(missing, &WalkerOptions::default()).expect_err("walk should fail");
    assert!(err.to_string().contains("invalid repository root"));
}

#[test]
fn walker_skips_binary_files_by_default() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text file");
    fixture
        .write_bytes("bin/data.bin", &[0, 1, 2, 3, 4, 5])
        .expect("write binary file");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);
    assert_eq!(paths, vec!["src/main.rs".to_string()]);
    assert_eq!(result.metrics.files_skipped_binary, 1);
}

#[test]
fn walker_applies_file_size_cap() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("small.txt", "12345")
        .expect("write small file");
    fixture
        .write("large.txt", "1234567890")
        .expect("write large file");

    let options = WalkerOptions {
        max_file_size_bytes: Some(5),
        ..WalkerOptions::default()
    };
    let result = walk_repository(fixture.path(), &options).expect("walk repo");
    let paths = relative_paths(&result.files);
    // Boundary behavior: exactly 5 bytes is included, >5 bytes is excluded.
    assert_eq!(paths, vec!["small.txt".to_string()]);
    assert_eq!(result.metrics.files_skipped_size, 1);
}

#[test]
fn walker_skips_symlinked_files() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text file");
    fixture
        .write("outside.txt", "outside")
        .expect("write outside file");
    fixture
        .symlink_file("outside.txt", "src/outside-link.txt")
        .expect("create symlink");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);
    assert_eq!(
        paths,
        vec!["outside.txt".to_string(), "src/main.rs".to_string()]
    );
    assert_eq!(result.metrics.files_skipped_symlink, 1);
}

#[test]
fn walker_skips_known_binary_extensions_even_without_nul_bytes() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("assets/image.png", "not-a-real-png-but-binary-by-extension")
        .expect("write extension-marked binary");
    fixture.write("README.md", "# Repo\n").expect("write file");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&result.files);
    assert_eq!(paths, vec!["README.md".to_string()]);
    assert_eq!(result.metrics.files_skipped_binary, 1);
}

#[test]
fn walker_enforces_file_count_limit() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture.write("a.txt", "a").expect("write file");
    fixture.write("b.txt", "b").expect("write file");

    let options = WalkerOptions {
        max_file_count: Some(1),
        ..WalkerOptions::default()
    };
    let err = walk_repository(fixture.path(), &options).expect_err("expected file count limit");
    assert!(err.to_string().contains("file_count"));
}

#[test]
fn metrics_reflect_all_skip_reasons() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    // Accepted files
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text");
    // Binary file (nul byte)
    fixture
        .write_bytes("bin/data.bin", &[0, 1, 2, 3])
        .expect("write binary");
    // Oversized file
    fixture
        .write("big.txt", &"x".repeat(200))
        .expect("write big");
    // Extra-rule-ignored file
    fixture
        .write("tmp/cache.txt", "cache")
        .expect("write ignored");
    // Symlinked file
    fixture.write("target.txt", "target").expect("write target");
    fixture
        .symlink_file("target.txt", "link.txt")
        .expect("create symlink");

    let options = WalkerOptions {
        extra_ignore_rules: vec!["tmp/**".to_string()],
        max_file_size_bytes: Some(100),
        ..WalkerOptions::default()
    };
    let result = walk_repository(fixture.path(), &options).expect("walk repo");

    assert_eq!(result.metrics.files_discovered, 2); // src/main.rs + target.txt
    assert_eq!(result.metrics.files_skipped_binary, 1);
    assert_eq!(result.metrics.files_skipped_size, 1);
    assert_eq!(result.metrics.files_skipped_extra_rules, 1);
    assert_eq!(result.metrics.files_skipped_symlink, 1);
    assert_eq!(result.metrics.total_entries_evaluated(), 6);
    assert!(result.metrics.walk_duration_ms < 10_000); // sanity: under 10s
}

fn relative_paths(results: &[repo_walker::DiscoveredFile]) -> Vec<String> {
    results
        .iter()
        .map(|item| item.relative_path.to_string_lossy().replace('\\', "/"))
        .collect()
}

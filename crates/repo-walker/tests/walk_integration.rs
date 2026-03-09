use std::fs;
use std::path::Path;

use repo_walker::{walk_repository, WalkerOptions};
use tempfile::TempDir;

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

    let results = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&results);

    assert_eq!(
        paths,
        vec![
            ".gitignore".to_string(),
            "README.md".to_string(),
            "src/main.rs".to_string(),
            "subdir/keep.tmp".to_string(),
        ]
    );
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
        include_git_dir: false,
        max_file_size_bytes: None,
        max_file_count: None,
        skip_binary_files: true,
    };
    let results = walk_repository(fixture.path(), &options).expect("walk repo");
    let paths = relative_paths(&results);

    assert_eq!(
        paths,
        vec!["README.md".to_string(), "src/main.rs".to_string()]
    );
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

    let results = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&results);

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

    let results = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&results);
    assert_eq!(paths, vec!["src/main.rs".to_string()]);
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
    let results = walk_repository(fixture.path(), &options).expect("walk repo");
    let paths = relative_paths(&results);
    // Boundary behavior: exactly 5 bytes is included, >5 bytes is excluded.
    assert_eq!(paths, vec!["small.txt".to_string()]);
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

    let results = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&results);
    assert_eq!(
        paths,
        vec!["outside.txt".to_string(), "src/main.rs".to_string()]
    );
}

#[test]
fn walker_skips_known_binary_extensions_even_without_nul_bytes() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("assets/image.png", "not-a-real-png-but-binary-by-extension")
        .expect("write extension-marked binary");
    fixture.write("README.md", "# Repo\n").expect("write file");

    let results = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let paths = relative_paths(&results);
    assert_eq!(paths, vec!["README.md".to_string()]);
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

fn relative_paths(results: &[repo_walker::DiscoveredFile]) -> Vec<String> {
    results
        .iter()
        .map(|item| item.relative_path.to_string_lossy().replace('\\', "/"))
        .collect()
}

struct FixtureRepo {
    tempdir: TempDir,
}

impl FixtureRepo {
    fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            tempdir: tempfile::tempdir()?,
        })
    }

    fn path(&self) -> &Path {
        self.tempdir.path()
    }

    fn write(&self, rel: &str, contents: &str) -> Result<(), std::io::Error> {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    fn write_bytes(&self, rel: &str, contents: &[u8]) -> Result<(), std::io::Error> {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    fn symlink_file(&self, source_rel: &str, target_rel: &str) -> Result<(), std::io::Error> {
        let source = self.path().join(source_rel);
        let target = self.path().join(target_rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        create_symlink_file(&source, &target)
    }
}

#[cfg(unix)]
fn create_symlink_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    std::os::windows::fs::symlink_file(source, target)
}

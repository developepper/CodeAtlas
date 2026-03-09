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
}

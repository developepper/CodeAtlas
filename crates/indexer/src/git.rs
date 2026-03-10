//! Git integration utilities for accelerated change detection.
//!
//! Uses the `git` CLI via [`std::process::Command`] to avoid adding a
//! heavy native git dependency. All functions gracefully return `None` or
//! an empty result when git is unavailable or the path is not a git repo.
//!
//! See spec §8.5 (git-diff accelerated mode).

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use tracing::debug;

/// Returns `true` if the given path is inside a git working tree.
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Returns the current HEAD commit hash (full 40-char SHA), or `None` if
/// not a git repo or git is unavailable.
pub fn current_head(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        None
    } else {
        Some(head)
    }
}

/// Returns the set of file paths with uncommitted changes (both staged and
/// unstaged) relative to the repository root.
///
/// Combines `git diff --name-only HEAD` (unstaged) and
/// `git diff --name-only --cached HEAD` (staged) to capture all working-tree
/// modifications not yet committed. Returns `None` if the git command fails.
pub fn dirty_files(path: &Path) -> Option<HashSet<String>> {
    // Unstaged changes (working tree vs index).
    let unstaged = Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    // Staged changes (index vs HEAD).
    let staged = Command::new("git")
        .args(["diff", "--name-only", "--cached", "HEAD"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !unstaged.status.success() || !staged.status.success() {
        debug!("git diff for dirty files failed, falling back to hash-based detection");
        return None;
    }

    let mut files = HashSet::new();
    for output in [&unstaged.stdout, &staged.stdout] {
        for line in String::from_utf8_lossy(output).lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    Some(files)
}

/// Returns the set of file paths that changed between two commits, relative
/// to the repository root.
///
/// Uses `git diff --name-only` which captures added, modified, deleted, and
/// renamed files. Returns `None` if the git command fails (e.g. invalid
/// commit, shallow clone missing history).
pub fn diff_files(path: &Path, from_commit: &str, to_commit: &str) -> Option<HashSet<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", from_commit, to_commit])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        debug!(
            from = from_commit,
            to = to_commit,
            "git diff failed, falling back to hash-based detection"
        );
        return None;
    }

    let files: HashSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    Some(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git init");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .status()
            .expect("git config email");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .status()
            .expect("git config name");
    }

    fn git_add_commit(dir: &Path, message: &str) {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .status()
            .expect("git add");

        Command::new("git")
            .args(["commit", "-m", message, "--allow-empty"])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git commit");
    }

    #[test]
    fn is_git_repo_returns_true_for_git_dir() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        assert!(is_git_repo(dir.path()));
    }

    #[test]
    fn is_git_repo_returns_false_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn current_head_returns_commit_hash() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();
        git_add_commit(dir.path(), "initial");

        let head = current_head(dir.path());
        assert!(head.is_some());
        let hash = head.unwrap();
        assert_eq!(hash.len(), 40, "should be full SHA: {hash}");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "should be hex: {hash}"
        );
    }

    #[test]
    fn current_head_returns_none_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        assert!(current_head(dir.path()).is_none());
    }

    #[test]
    fn diff_files_detects_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Commit 1: one file.
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        git_add_commit(dir.path(), "commit 1");
        let head1 = current_head(dir.path()).unwrap();

        // Commit 2: modify a.rs, add b.rs.
        std::fs::write(dir.path().join("a.rs"), "fn a_v2() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").unwrap();
        git_add_commit(dir.path(), "commit 2");
        let head2 = current_head(dir.path()).unwrap();

        let changed = diff_files(dir.path(), &head1, &head2);
        assert!(changed.is_some());
        let changed = changed.unwrap();
        assert!(changed.contains("a.rs"), "a.rs was modified");
        assert!(changed.contains("b.rs"), "b.rs was added");
    }

    #[test]
    fn diff_files_detects_deletion() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").unwrap();
        git_add_commit(dir.path(), "commit 1");
        let head1 = current_head(dir.path()).unwrap();

        std::fs::remove_file(dir.path().join("b.rs")).unwrap();
        git_add_commit(dir.path(), "commit 2");
        let head2 = current_head(dir.path()).unwrap();

        let changed = diff_files(dir.path(), &head1, &head2).unwrap();
        assert!(changed.contains("b.rs"), "b.rs was deleted");
        assert!(!changed.contains("a.rs"), "a.rs was unchanged");
    }

    #[test]
    fn diff_files_returns_none_for_invalid_commit() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        git_add_commit(dir.path(), "init");

        assert!(diff_files(
            dir.path(),
            "0000000000000000000000000000000000000000",
            "HEAD"
        )
        .is_none());
    }

    #[test]
    fn diff_files_returns_none_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        assert!(diff_files(dir.path(), "abc", "def").is_none());
    }

    #[test]
    fn dirty_files_detects_unstaged_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").unwrap();
        git_add_commit(dir.path(), "initial");

        // Modify a.rs without committing.
        std::fs::write(dir.path().join("a.rs"), "fn a_v2() {}").unwrap();

        let dirty = dirty_files(dir.path()).unwrap();
        assert!(dirty.contains("a.rs"), "a.rs was modified in working tree");
        assert!(!dirty.contains("b.rs"), "b.rs was not touched");
    }

    #[test]
    fn dirty_files_detects_staged_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        git_add_commit(dir.path(), "initial");

        // Stage a change without committing.
        std::fs::write(dir.path().join("a.rs"), "fn a_staged() {}").unwrap();
        Command::new("git")
            .args(["add", "a.rs"])
            .current_dir(dir.path())
            .stdout(std::process::Stdio::null())
            .status()
            .expect("git add");

        let dirty = dirty_files(dir.path()).unwrap();
        assert!(dirty.contains("a.rs"), "a.rs was staged");
    }

    #[test]
    fn dirty_files_returns_empty_when_clean() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        git_add_commit(dir.path(), "initial");

        let dirty = dirty_files(dir.path()).unwrap();
        assert!(dirty.is_empty(), "no dirty files expected");
    }

    #[test]
    fn dirty_files_returns_none_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        assert!(dirty_files(dir.path()).is_none());
    }
}

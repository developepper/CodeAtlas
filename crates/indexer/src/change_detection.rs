//! File-level change detection for incremental indexing.
//!
//! Compares discovered files against a persisted hash map (from the metadata
//! store) to classify each file as changed, new, unchanged, or deleted.
//!
//! See spec §8.5 (Incremental Indexing).

use std::collections::{HashMap, HashSet};

use syntax_platform::PreparedFile;

/// Classification of discovered files relative to the previous index.
#[derive(Debug)]
pub struct ChangeSet {
    /// Indices into the discovery file list for files that are new or changed.
    pub changed_indices: Vec<usize>,
    /// Number of files that were unchanged (hash match).
    pub unchanged_count: usize,
    /// Number of files that are new (not in previous index).
    pub new_count: usize,
    /// Number of files whose content hash changed.
    pub modified_count: usize,
    /// Paths of files present in the previous index but absent from the
    /// current discovery (i.e. deleted from disk). The persist stage removes
    /// these via `delete_except`; this field surfaces them for metrics and
    /// telemetry.
    pub deleted_paths: Vec<String>,
}

/// Computes which discovered files need re-indexing by comparing their content
/// hashes against the previously persisted hash map.
///
/// - Files whose path is absent from `previous_hashes` are **new**.
/// - Files whose hash differs from `previous_hashes` are **modified**.
/// - Files whose hash matches are **unchanged** and can be skipped.
/// - Files in `previous_hashes` but not in `files` are **deleted**.
///
/// Returns a [`ChangeSet`] with indices into `files` for changed/new entries,
/// plus the list of deleted paths.
pub fn detect_changes(
    files: &[PreparedFile],
    previous_hashes: &HashMap<String, String>,
) -> ChangeSet {
    let mut changed_indices = Vec::new();
    let mut unchanged_count = 0usize;
    let mut new_count = 0usize;
    let mut modified_count = 0usize;

    for (i, file) in files.iter().enumerate() {
        let path = file.relative_path.to_string_lossy();
        let current_hash = store::content_hash(&file.content);

        match previous_hashes.get(path.as_ref()) {
            None => {
                new_count += 1;
                changed_indices.push(i);
            }
            Some(prev_hash) if *prev_hash != current_hash => {
                modified_count += 1;
                changed_indices.push(i);
            }
            Some(_) => {
                unchanged_count += 1;
            }
        }
    }

    // Compute deleted paths: in previous_hashes but not in discovered files.
    let discovered_set: HashSet<String> = files
        .iter()
        .map(|f| f.relative_path.to_string_lossy().into_owned())
        .collect();

    let mut deleted_paths: Vec<String> = previous_hashes
        .keys()
        .filter(|p| !discovered_set.contains(p.as_str()))
        .cloned()
        .collect();
    deleted_paths.sort();

    ChangeSet {
        changed_indices,
        unchanged_count,
        new_count,
        modified_count,
        deleted_paths,
    }
}

/// Git-diff accelerated change detection.
///
/// Uses `git diff --name-only` between the previously stored HEAD and the
/// current HEAD to identify committed changes, **plus** `git diff` against
/// HEAD for uncommitted working-tree changes (staged and unstaged). This
/// ensures that files edited without committing are still detected.
///
/// Returns `None` if git-diff is unavailable (not a git repo, missing
/// commits, etc.), signaling the caller to fall back to hash-based detection.
pub fn detect_changes_git(
    files: &[PreparedFile],
    previous_hashes: &HashMap<String, String>,
    source_root: &std::path::Path,
    previous_head: &str,
    current_head: &str,
) -> Option<ChangeSet> {
    // Always collect uncommitted working-tree changes so edits that have
    // not been committed are never silently skipped.
    let dirty = crate::git::dirty_files(source_root)?;

    let committed_changes = if previous_head == current_head {
        // Same commit — no committed changes between the two.
        HashSet::new()
    } else {
        crate::git::diff_files(source_root, previous_head, current_head)?
    };

    // Union of committed inter-commit changes and uncommitted dirty files.
    let git_changed: HashSet<&str> = committed_changes
        .iter()
        .map(|s| s.as_str())
        .chain(dirty.iter().map(|s| s.as_str()))
        .collect();

    let discovered_set: HashSet<String> = files
        .iter()
        .map(|f| f.relative_path.to_string_lossy().into_owned())
        .collect();

    let mut changed_indices = Vec::new();
    let mut unchanged_count = 0usize;
    let mut new_count = 0usize;
    let mut modified_count = 0usize;

    for (i, file) in files.iter().enumerate() {
        let path = file.relative_path.to_string_lossy();

        if !previous_hashes.contains_key(path.as_ref()) {
            // New file.
            new_count += 1;
            changed_indices.push(i);
        } else if git_changed.contains(path.as_ref()) {
            // Git says this file changed (committed or uncommitted).
            modified_count += 1;
            changed_indices.push(i);
        } else {
            unchanged_count += 1;
        }
    }

    let mut deleted_paths: Vec<String> = previous_hashes
        .keys()
        .filter(|p| !discovered_set.contains(p.as_str()))
        .cloned()
        .collect();
    deleted_paths.sort();

    Some(ChangeSet {
        changed_indices,
        unchanged_count,
        new_count,
        modified_count,
        deleted_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(path: &str, content: &[u8]) -> PreparedFile {
        PreparedFile {
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from("/tmp").join(path),
            language: "rust".to_string(),
            content: content.to_vec(),
        }
    }

    #[test]
    fn all_new_files_when_no_previous_hashes() {
        let files = vec![
            make_file("src/main.rs", b"fn main() {}"),
            make_file("src/lib.rs", b"pub fn foo() {}"),
        ];
        let previous = HashMap::new();

        let cs = detect_changes(&files, &previous);
        assert_eq!(cs.changed_indices, vec![0, 1]);
        assert_eq!(cs.new_count, 2);
        assert_eq!(cs.modified_count, 0);
        assert_eq!(cs.unchanged_count, 0);
        assert!(cs.deleted_paths.is_empty());
    }

    #[test]
    fn unchanged_files_skipped() {
        let content = b"fn main() {}";
        let hash = store::content_hash(content);
        let files = vec![make_file("src/main.rs", content)];
        let mut previous = HashMap::new();
        previous.insert("src/main.rs".to_string(), hash);

        let cs = detect_changes(&files, &previous);
        assert!(cs.changed_indices.is_empty());
        assert_eq!(cs.unchanged_count, 1);
        assert_eq!(cs.new_count, 0);
        assert_eq!(cs.modified_count, 0);
        assert!(cs.deleted_paths.is_empty());
    }

    #[test]
    fn modified_file_detected() {
        let files = vec![make_file("src/main.rs", b"fn main() { updated }")];
        let mut previous = HashMap::new();
        previous.insert(
            "src/main.rs".to_string(),
            store::content_hash(b"fn main() {}"),
        );

        let cs = detect_changes(&files, &previous);
        assert_eq!(cs.changed_indices, vec![0]);
        assert_eq!(cs.modified_count, 1);
        assert_eq!(cs.new_count, 0);
        assert_eq!(cs.unchanged_count, 0);
        assert!(cs.deleted_paths.is_empty());
    }

    #[test]
    fn deleted_files_detected() {
        let content = b"fn main() {}";
        let hash = store::content_hash(content);
        let files = vec![make_file("src/main.rs", content)];
        let mut previous = HashMap::new();
        previous.insert("src/main.rs".to_string(), hash);
        previous.insert("src/old.rs".to_string(), "oldhash".to_string());
        previous.insert("src/gone.rs".to_string(), "gonehash".to_string());

        let cs = detect_changes(&files, &previous);
        assert!(cs.changed_indices.is_empty());
        assert_eq!(cs.unchanged_count, 1);
        assert_eq!(cs.deleted_paths, vec!["src/gone.rs", "src/old.rs"]);
    }

    #[test]
    fn mixed_new_modified_unchanged_deleted() {
        let unchanged_content = b"unchanged";
        let unchanged_hash = store::content_hash(unchanged_content);

        let files = vec![
            make_file("a.rs", unchanged_content),   // unchanged
            make_file("b.rs", b"modified content"), // modified
            make_file("c.rs", b"brand new"),        // new
        ];

        let mut previous = HashMap::new();
        previous.insert("a.rs".to_string(), unchanged_hash);
        previous.insert("b.rs".to_string(), store::content_hash(b"old content"));
        previous.insert("d.rs".to_string(), "deleted_hash".to_string());
        // c.rs not in previous — it's new
        // d.rs in previous but not in discovery — deleted

        let cs = detect_changes(&files, &previous);
        assert_eq!(cs.changed_indices, vec![1, 2]);
        assert_eq!(cs.unchanged_count, 1);
        assert_eq!(cs.modified_count, 1);
        assert_eq!(cs.new_count, 1);
        assert_eq!(cs.deleted_paths, vec!["d.rs"]);
    }

    #[test]
    fn empty_discovery_detects_all_as_deleted() {
        let files: Vec<PreparedFile> = vec![];
        let mut previous = HashMap::new();
        previous.insert("old.rs".to_string(), "somehash".to_string());

        let cs = detect_changes(&files, &previous);
        assert!(cs.changed_indices.is_empty());
        assert_eq!(cs.unchanged_count, 0);
        assert_eq!(cs.new_count, 0);
        assert_eq!(cs.modified_count, 0);
        assert_eq!(cs.deleted_paths, vec!["old.rs"]);
    }

    #[test]
    fn deleted_paths_are_sorted() {
        let files: Vec<PreparedFile> = vec![];
        let mut previous = HashMap::new();
        previous.insert("z.rs".to_string(), "h1".to_string());
        previous.insert("a.rs".to_string(), "h2".to_string());
        previous.insert("m.rs".to_string(), "h3".to_string());

        let cs = detect_changes(&files, &previous);
        assert_eq!(cs.deleted_paths, vec!["a.rs", "m.rs", "z.rs"]);
    }
}

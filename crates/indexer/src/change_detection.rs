//! File-level change detection for incremental indexing.
//!
//! Compares discovered files against a persisted hash map (from the metadata
//! store) to classify each file as changed, new, or unchanged. Files present
//! in the hash map but absent from discovery are implicitly deleted — handled
//! by the existing `delete_except` logic in the persist stage.
//!
//! See spec §8.5 (Incremental Indexing).

use std::collections::HashMap;

use crate::stage::PreparedFile;

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
}

/// Computes which discovered files need re-indexing by comparing their content
/// hashes against the previously persisted hash map.
///
/// - Files whose path is absent from `previous_hashes` are **new**.
/// - Files whose hash differs from `previous_hashes` are **modified**.
/// - Files whose hash matches are **unchanged** and can be skipped.
///
/// Returns a [`ChangeSet`] with indices into `files` for changed/new entries.
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
                // New file — not in previous index.
                new_count += 1;
                changed_indices.push(i);
            }
            Some(prev_hash) if *prev_hash != current_hash => {
                // Modified file — hash changed.
                modified_count += 1;
                changed_indices.push(i);
            }
            Some(_) => {
                // Unchanged — skip re-indexing.
                unchanged_count += 1;
            }
        }
    }

    ChangeSet {
        changed_indices,
        unchanged_count,
        new_count,
        modified_count,
    }
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
    }

    #[test]
    fn mixed_new_modified_unchanged() {
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
        // c.rs not in previous — it's new
        // d.rs was in previous but not in discovery — implicitly deleted

        let cs = detect_changes(&files, &previous);
        assert_eq!(cs.changed_indices, vec![1, 2]);
        assert_eq!(cs.unchanged_count, 1);
        assert_eq!(cs.modified_count, 1);
        assert_eq!(cs.new_count, 1);
    }

    #[test]
    fn empty_discovery_produces_empty_changeset() {
        let files: Vec<PreparedFile> = vec![];
        let mut previous = HashMap::new();
        previous.insert("old.rs".to_string(), "somehash".to_string());

        let cs = detect_changes(&files, &previous);
        assert!(cs.changed_indices.is_empty());
        assert_eq!(cs.unchanged_count, 0);
        assert_eq!(cs.new_count, 0);
        assert_eq!(cs.modified_count, 0);
    }
}

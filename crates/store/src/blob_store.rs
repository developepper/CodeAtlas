//! Content-addressed blob storage backed by the local filesystem.
//!
//! Blobs are keyed by their SHA-256 hex digest and stored in a sharded
//! directory layout (`ab/cdef0123...`) to avoid single-directory inode
//! pressure. Writing a blob whose hash already exists on disk is a
//! deduplicated no-op.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::StoreError;

/// Computes the SHA-256 hex digest of the given content.
///
/// This is the canonical hash function for all content addressing in
/// CodeAtlas. Both the blob store and the indexer pipeline must use this
/// function to produce `file_hash` / `content_hash` values.
#[must_use]
pub fn content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Filesystem-backed content-addressed blob store.
///
/// Blobs are stored under a root directory with a two-character shard
/// prefix derived from the first two hex characters of the SHA-256 digest:
///
/// ```text
/// <root>/ab/abcdef0123456789...
/// ```
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// Opens (or creates) a blob store at the given root directory.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        fs::create_dir_all(root).map_err(|e| StoreError::Blob {
            path: Some(root.to_path_buf()),
            reason: format!("failed to create blob store root: {e}"),
        })?;

        let root = root.canonicalize().map_err(|e| StoreError::Blob {
            path: Some(root.to_path_buf()),
            reason: format!("failed to canonicalize blob store root: {e}"),
        })?;

        Ok(Self { root })
    }

    /// Returns the root directory of the blob store.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stores content and returns its SHA-256 hex digest.
    ///
    /// If a blob with the same hash already exists, the write is skipped
    /// (content-addressed deduplication). The returned hash is always the
    /// canonical SHA-256 digest regardless of whether the blob was newly
    /// written or already present.
    pub fn put(&self, content: &[u8]) -> Result<String, StoreError> {
        let hash = content_hash(content);
        let blob_path = self.blob_path(&hash);

        if blob_path.exists() {
            return Ok(hash);
        }

        let shard_dir = blob_path.parent().expect("blob path always has a parent");
        fs::create_dir_all(shard_dir).map_err(|e| StoreError::Blob {
            path: Some(shard_dir.to_path_buf()),
            reason: format!("failed to create shard directory: {e}"),
        })?;

        // Write to a temporary file then rename for atomic visibility.
        let tmp_path = blob_path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path).map_err(|e| StoreError::Blob {
            path: Some(tmp_path.clone()),
            reason: format!("failed to create temp blob file: {e}"),
        })?;

        file.write_all(content).map_err(|e| StoreError::Blob {
            path: Some(tmp_path.clone()),
            reason: format!("failed to write blob content: {e}"),
        })?;

        file.flush().map_err(|e| StoreError::Blob {
            path: Some(tmp_path.clone()),
            reason: format!("failed to flush blob file: {e}"),
        })?;

        fs::rename(&tmp_path, &blob_path).map_err(|e| StoreError::Blob {
            path: Some(blob_path),
            reason: format!("failed to rename temp file to blob: {e}"),
        })?;

        Ok(hash)
    }

    /// Retrieves the content for the given hash.
    ///
    /// Returns `None` if no blob with that hash exists.
    pub fn get(&self, hash: &str) -> Result<Option<Vec<u8>>, StoreError> {
        validate_hash(hash)?;
        let blob_path = self.blob_path(hash);

        if !blob_path.exists() {
            return Ok(None);
        }

        let content = fs::read(&blob_path).map_err(|e| StoreError::Blob {
            path: Some(blob_path),
            reason: format!("failed to read blob: {e}"),
        })?;

        Ok(Some(content))
    }

    /// Returns whether a blob with the given hash exists.
    pub fn exists(&self, hash: &str) -> Result<bool, StoreError> {
        validate_hash(hash)?;
        Ok(self.blob_path(hash).exists())
    }

    /// Deletes the blob with the given hash.
    ///
    /// Returns `true` if the blob existed and was removed, `false` if it
    /// did not exist.
    pub fn delete(&self, hash: &str) -> Result<bool, StoreError> {
        validate_hash(hash)?;
        let blob_path = self.blob_path(hash);

        if !blob_path.exists() {
            return Ok(false);
        }

        fs::remove_file(&blob_path).map_err(|e| StoreError::Blob {
            path: Some(blob_path),
            reason: format!("failed to delete blob: {e}"),
        })?;

        Ok(true)
    }

    /// Computes the filesystem path for a given hash.
    ///
    /// Layout: `<root>/<first-2-hex-chars>/<full-hash>`
    fn blob_path(&self, hash: &str) -> PathBuf {
        let shard = &hash[..2];
        self.root.join(shard).join(hash)
    }
}

/// Validates that a hash string looks like a valid SHA-256 hex digest.
fn validate_hash(hash: &str) -> Result<(), StoreError> {
    if hash.len() != 64 {
        return Err(StoreError::Blob {
            path: None,
            reason: format!(
                "invalid hash length: expected 64 hex characters, got {}",
                hash.len()
            ),
        });
    }
    if !hash
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StoreError::Blob {
            path: None,
            reason: "hash must be lowercase hex (0-9, a-f)".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, BlobStore) {
        let dir = TempDir::new().expect("create temp dir");
        let store = BlobStore::open(dir.path().join("blobs").as_path()).expect("open blob store");
        (dir, store)
    }

    #[test]
    fn content_hash_is_sha256() {
        let hash = content_hash(b"hello world");
        // Known SHA-256 digest for "hello world".
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn content_hash_is_deterministic() {
        let a = content_hash(b"test data");
        let b = content_hash(b"test data");
        assert_eq!(a, b);
    }

    #[test]
    fn content_hash_empty_input() {
        let hash = content_hash(b"");
        // Known SHA-256 digest for empty input.
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn put_and_get_round_trips() {
        let (_dir, store) = setup();
        let content = b"fn main() {}\n";

        let hash = store.put(content).expect("put blob");
        assert_eq!(hash.len(), 64);

        let retrieved = store.get(&hash).expect("get blob").expect("blob exists");
        assert_eq!(retrieved, content);
    }

    #[test]
    fn put_empty_content() {
        let (_dir, store) = setup();
        let hash = store.put(b"").expect("put empty blob");
        assert_eq!(hash, content_hash(b""));

        let retrieved = store.get(&hash).expect("get blob").expect("blob exists");
        assert!(retrieved.is_empty());
    }

    #[test]
    fn put_deduplicates_identical_content() {
        let (_dir, store) = setup();
        let content = b"deduplicated";

        let hash1 = store.put(content).expect("first put");
        let hash2 = store.put(content).expect("second put");
        assert_eq!(hash1, hash2);

        // Blob path should exist exactly once.
        let blob_path = store.blob_path(&hash1);
        assert!(blob_path.exists());
    }

    #[test]
    fn get_returns_none_for_missing_blob() {
        let (_dir, store) = setup();
        let hash = content_hash(b"nonexistent");
        let result = store.get(&hash).expect("get missing blob");
        assert!(result.is_none());
    }

    #[test]
    fn exists_returns_correct_status() {
        let (_dir, store) = setup();
        let hash = store.put(b"check existence").expect("put blob");

        assert!(store.exists(&hash).expect("exists check"));

        let missing = content_hash(b"not stored");
        assert!(!store.exists(&missing).expect("exists check"));
    }

    #[test]
    fn delete_removes_blob() {
        let (_dir, store) = setup();
        let hash = store.put(b"to be deleted").expect("put blob");

        assert!(store.delete(&hash).expect("delete blob"));
        assert!(!store.exists(&hash).expect("exists after delete"));
        assert!(store.get(&hash).expect("get after delete").is_none());
    }

    #[test]
    fn delete_returns_false_for_missing() {
        let (_dir, store) = setup();
        let hash = content_hash(b"never stored");
        assert!(!store.delete(&hash).expect("delete missing"));
    }

    #[test]
    fn blob_path_uses_shard_directory() {
        let (_dir, store) = setup();
        let hash = content_hash(b"shard test");
        let path = store.blob_path(&hash);

        let shard = &hash[..2];
        assert!(path.to_string_lossy().contains(&format!("/{shard}/")));
        assert!(path.to_string_lossy().ends_with(&hash));
    }

    #[test]
    fn different_content_produces_different_hashes() {
        let (_dir, store) = setup();
        let hash1 = store.put(b"content a").expect("put a");
        let hash2 = store.put(b"content b").expect("put b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn validate_hash_rejects_short_string() {
        let err = validate_hash("abcd").expect_err("should reject short hash");
        assert!(err.to_string().contains("invalid hash length"));
    }

    #[test]
    fn validate_hash_rejects_non_hex() {
        let bad = "g".repeat(64);
        let err = validate_hash(&bad).expect_err("should reject non-hex");
        assert!(err.to_string().contains("lowercase hex"));
    }

    #[test]
    fn validate_hash_rejects_uppercase_hex() {
        let upper = "A".repeat(64);
        let err = validate_hash(&upper).expect_err("should reject uppercase hex");
        assert!(err.to_string().contains("lowercase hex"));
    }

    #[test]
    fn validate_hash_accepts_valid_sha256() {
        let hash = content_hash(b"valid");
        validate_hash(&hash).expect("valid hash should pass");
    }
}

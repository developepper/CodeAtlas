//! Integration tests for the content-addressed blob store.

use store::{content_hash, BlobStore};
use tempfile::TempDir;

fn setup() -> (TempDir, BlobStore) {
    let dir = TempDir::new().expect("create temp dir");
    let store = BlobStore::open(&dir.path().join("blobs")).expect("open blob store");
    (dir, store)
}

#[test]
fn full_lifecycle_put_get_exists_delete() {
    let (_dir, store) = setup();
    let content = b"fn main() { println!(\"hello\"); }\n";

    // Put
    let hash = store.put(content).expect("put blob");
    assert_eq!(hash, content_hash(content));

    // Exists
    assert!(store.exists(&hash).expect("exists check"));

    // Get
    let retrieved = store.get(&hash).expect("get blob").expect("blob exists");
    assert_eq!(retrieved, content);

    // Delete
    assert!(store.delete(&hash).expect("delete blob"));
    assert!(!store.exists(&hash).expect("exists after delete"));
    assert!(store.get(&hash).expect("get after delete").is_none());
    assert!(!store.delete(&hash).expect("double delete"));
}

#[test]
fn deduplication_across_multiple_puts() {
    let (_dir, store) = setup();
    let content = b"shared content";

    let hash1 = store.put(content).expect("first put");
    let hash2 = store.put(content).expect("second put");
    let hash3 = store.put(content).expect("third put");

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);

    // Only one blob on disk — deleting once should suffice.
    assert!(store.delete(&hash1).expect("delete"));
    assert!(!store.exists(&hash1).expect("exists after delete"));
}

#[test]
fn distinct_content_produces_distinct_blobs() {
    let (_dir, store) = setup();

    let hash_a = store.put(b"alpha").expect("put alpha");
    let hash_b = store.put(b"beta").expect("put beta");
    assert_ne!(hash_a, hash_b);

    let a = store.get(&hash_a).expect("get a").expect("a exists");
    let b = store.get(&hash_b).expect("get b").expect("b exists");
    assert_eq!(a, b"alpha");
    assert_eq!(b, b"beta");
}

#[test]
fn empty_content_is_stored_and_retrievable() {
    let (_dir, store) = setup();

    let hash = store.put(b"").expect("put empty");
    assert!(store.exists(&hash).expect("exists"));

    let retrieved = store.get(&hash).expect("get").expect("exists");
    assert!(retrieved.is_empty());
}

#[test]
fn hash_function_matches_blob_store_keying() {
    let (_dir, store) = setup();
    let content = b"consistency check";

    let standalone_hash = content_hash(content);
    let store_hash = store.put(content).expect("put");

    assert_eq!(standalone_hash, store_hash);
}

#[test]
fn invalid_hash_is_rejected() {
    let (_dir, store) = setup();

    let err = store.get("not-a-hash").expect_err("should reject bad hash");
    assert!(err.to_string().contains("invalid hash length"));

    let err = store.exists("abcd").expect_err("should reject short hash");
    assert!(err.to_string().contains("invalid hash length"));

    // Uppercase hex is rejected to prevent silent path mismatches.
    let upper = "A".repeat(64);
    let err = store.get(&upper).expect_err("should reject uppercase");
    assert!(err.to_string().contains("lowercase hex"));
}

#[test]
fn blob_store_persists_across_reopen() {
    let dir = TempDir::new().expect("create temp dir");
    let blob_dir = dir.path().join("blobs");

    let hash = {
        let store = BlobStore::open(&blob_dir).expect("open store");
        store.put(b"persistent data").expect("put blob")
    };

    // Reopen the store and verify the blob persists.
    let store = BlobStore::open(&blob_dir).expect("reopen store");
    let retrieved = store.get(&hash).expect("get blob").expect("blob exists");
    assert_eq!(retrieved, b"persistent data");
}

//! Integration tests for the metadata store using temporary SQLite databases.

use std::collections::BTreeMap;

use core_model::{FileRecord, QualityLevel, QualityMix, RepoRecord, SymbolKind, SymbolRecord};
use store::MetadataStore;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_repo() -> RepoRecord {
    let mut language_counts = BTreeMap::new();
    language_counts.insert("rust".to_string(), 10);

    RepoRecord {
        repo_id: "integration-repo".to_string(),
        display_name: "Integration Test Repo".to_string(),
        source_root: "/home/user/repos/integration".to_string(),
        indexed_at: "2025-01-15T10:30:00Z".to_string(),
        index_version: "1.0.0".to_string(),
        language_counts,
        file_count: 3,
        symbol_count: 42,
        git_head: Some("deadbeef".to_string()),
    }
}

fn test_file(file_path: &str) -> FileRecord {
    FileRecord {
        repo_id: "integration-repo".to_string(),
        file_path: file_path.to_string(),
        language: "rust".to_string(),
        file_hash: format!("sha256:{file_path}"),
        summary: format!("Summary for {file_path}"),
        symbol_count: 3,
        quality_mix: QualityMix {
            semantic_percent: 0.0,
            syntax_percent: 100.0,
        },
        updated_at: "2025-01-15T10:30:00Z".to_string(),
    }
}

fn test_symbol(file_path: &str, name: &str, kind: SymbolKind) -> SymbolRecord {
    SymbolRecord {
        id: format!("{file_path}::{name}#{}", kind.as_str()),
        repo_id: "integration-repo".to_string(),
        file_path: file_path.to_string(),
        language: "rust".to_string(),
        kind,
        name: name.to_string(),
        qualified_name: name.to_string(),
        signature: format!("fn {name}()"),
        start_line: 1,
        end_line: 5,
        start_byte: 0,
        byte_length: 50,
        content_hash: "sha256:content".to_string(),
        quality_level: QualityLevel::Syntax,
        confidence_score: 0.7,
        source_adapter: "syntax-treesitter-rust".to_string(),
        indexed_at: "2025-01-15T10:30:00Z".to_string(),
        docstring: None,
        summary: None,
        parent_symbol_id: None,
        keywords: None,
        decorators_or_attributes: None,
        semantic_refs: None,
    }
}

// ---------------------------------------------------------------------------
// File-backed database
// ---------------------------------------------------------------------------

#[test]
fn open_creates_database_on_disk() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");

    let store = MetadataStore::open(&db_path).unwrap();
    assert_eq!(store.schema_version().unwrap(), store::SCHEMA_VERSION);
    assert!(db_path.exists());
}

#[test]
fn reopen_existing_database_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");

    {
        let store = MetadataStore::open(&db_path).unwrap();
        store.repos().upsert(&test_repo()).unwrap();
    }

    // Reopen and verify data persists.
    let store = MetadataStore::open(&db_path).unwrap();
    let repo = store.repos().get("integration-repo").unwrap();
    assert!(repo.is_some());
    assert_eq!(repo.unwrap().display_name, "Integration Test Repo");
}

// ---------------------------------------------------------------------------
// End-to-end: repo -> files -> symbols lifecycle
// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_create_read_update_delete() {
    let store = MetadataStore::open_in_memory().unwrap();

    // Create repo.
    store.repos().upsert(&test_repo()).unwrap();
    assert_eq!(store.repos().list_ids().unwrap(), vec!["integration-repo"]);

    // Create files.
    store.files().upsert(&test_file("src/lib.rs")).unwrap();
    store.files().upsert(&test_file("src/main.rs")).unwrap();
    assert_eq!(
        store.files().list_paths("integration-repo").unwrap(),
        vec!["src/lib.rs", "src/main.rs"]
    );

    // Create symbols.
    store
        .symbols()
        .upsert(&test_symbol("src/lib.rs", "Config", SymbolKind::Class))
        .unwrap();
    store
        .symbols()
        .upsert(&test_symbol("src/lib.rs", "new", SymbolKind::Method))
        .unwrap();
    store
        .symbols()
        .upsert(&test_symbol("src/main.rs", "main", SymbolKind::Function))
        .unwrap();

    // Read back symbols for a file.
    let lib_syms = store
        .symbols()
        .list_ids_for_file("integration-repo", "src/lib.rs")
        .unwrap();
    assert_eq!(lib_syms.len(), 2);

    // Update a symbol.
    let mut sym = test_symbol("src/lib.rs", "Config", SymbolKind::Class);
    sym.confidence_score = 0.95;
    store.symbols().upsert(&sym).unwrap();
    let loaded = store
        .symbols()
        .get("src/lib.rs::Config#class")
        .unwrap()
        .unwrap();
    assert!((loaded.confidence_score - 0.95).abs() < f32::EPSILON);

    // Delete file cascades to symbols.
    store
        .files()
        .delete("integration-repo", "src/lib.rs")
        .unwrap();
    assert!(store
        .symbols()
        .list_ids_for_file("integration-repo", "src/lib.rs")
        .unwrap()
        .is_empty());

    // Delete repo cascades to remaining files/symbols.
    store.repos().delete("integration-repo").unwrap();
    assert!(store.repos().list_ids().unwrap().is_empty());
    assert!(store
        .files()
        .list_paths("integration-repo")
        .unwrap()
        .is_empty());
}

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

#[test]
fn schema_version_matches_constant() {
    let store = MetadataStore::open_in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), store::SCHEMA_VERSION);
}

// ---------------------------------------------------------------------------
// Rollback
// ---------------------------------------------------------------------------

#[test]
fn rollback_to_zero_and_reapply_on_disk() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("rollback.db");

    // Create and populate.
    {
        let store = MetadataStore::open(&db_path).unwrap();
        store.repos().upsert(&test_repo()).unwrap();
        assert_eq!(store.schema_version().unwrap(), store::SCHEMA_VERSION);

        // Rollback to 0 using the public conn() accessor.
        store::rollback_to(store.conn(), 0).unwrap();
        assert_eq!(store.schema_version().unwrap(), 0);
    }

    // Reopen applies migrations again.
    {
        let store = MetadataStore::open(&db_path).unwrap();
        assert_eq!(store.schema_version().unwrap(), store::SCHEMA_VERSION);
        // Data is gone after rollback + table recreation.
        assert!(store.repos().get("integration-repo").unwrap().is_none());
    }
}

// ---------------------------------------------------------------------------
// Transaction: atomic commit
// ---------------------------------------------------------------------------

#[test]
fn transaction_commit_persists_all_writes() {
    let mut store = MetadataStore::open_in_memory().unwrap();

    {
        let tx = store.transaction().unwrap();
        tx.repos().upsert(&test_repo()).unwrap();
        tx.files().upsert(&test_file("src/lib.rs")).unwrap();
        tx.symbols()
            .upsert(&test_symbol("src/lib.rs", "Config", SymbolKind::Class))
            .unwrap();
        tx.commit().unwrap();
    }

    // All three records are visible after commit.
    assert!(store.repos().get("integration-repo").unwrap().is_some());
    assert!(store
        .files()
        .get("integration-repo", "src/lib.rs")
        .unwrap()
        .is_some());
    assert!(store
        .symbols()
        .get("src/lib.rs::Config#class")
        .unwrap()
        .is_some());
}

#[test]
fn transaction_drop_without_commit_rolls_back() {
    let mut store = MetadataStore::open_in_memory().unwrap();

    // Pre-populate a repo so we can verify it survives the rollback.
    store.repos().upsert(&test_repo()).unwrap();

    {
        let tx = store.transaction().unwrap();
        tx.files().upsert(&test_file("src/lib.rs")).unwrap();
        tx.symbols()
            .upsert(&test_symbol("src/lib.rs", "Config", SymbolKind::Class))
            .unwrap();
        // Drop without commit — automatic rollback.
    }

    // Repo still exists (was committed before the transaction).
    assert!(store.repos().get("integration-repo").unwrap().is_some());
    // File and symbol were never committed.
    assert!(store
        .files()
        .get("integration-repo", "src/lib.rs")
        .unwrap()
        .is_none());
    assert!(store
        .symbols()
        .get("src/lib.rs::Config#class")
        .unwrap()
        .is_none());
}

#[test]
fn transaction_rollback_on_error_leaves_store_unchanged() {
    let mut store = MetadataStore::open_in_memory().unwrap();

    // Attempt a transaction that inserts a repo then hits a validation error
    // on a symbol. The transaction is dropped, so all writes roll back.
    let result: Result<(), store::StoreError> = (|| {
        let tx = store.transaction()?;
        tx.repos().upsert(&test_repo())?;
        tx.files().upsert(&test_file("src/main.rs"))?;

        // Symbol with empty name fails validation.
        let mut bad_sym = test_symbol("src/main.rs", "main", SymbolKind::Function);
        bad_sym.name = "".to_string();
        tx.symbols().upsert(&bad_sym)?;

        tx.commit()?;
        Ok(())
    })();

    assert!(result.is_err());
    // Nothing was persisted because the transaction was dropped.
    assert!(store.repos().get("integration-repo").unwrap().is_none());
}

#[test]
fn transaction_crash_retry_simulation() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("crash_sim.db");

    // First attempt: open, begin transaction, write, then "crash" (drop without commit).
    {
        let mut store = MetadataStore::open(&db_path).unwrap();
        let tx = store.transaction().unwrap();
        tx.repos().upsert(&test_repo()).unwrap();
        tx.files().upsert(&test_file("src/lib.rs")).unwrap();
        // Simulate crash: drop tx without commit.
        drop(tx);
        // Verify nothing persisted in this session.
        assert!(store.repos().get("integration-repo").unwrap().is_none());
    }

    // Second attempt (retry): reopen, transact, commit successfully.
    {
        let mut store = MetadataStore::open(&db_path).unwrap();
        // Database is clean — no partial state from the "crash".
        assert!(store.repos().get("integration-repo").unwrap().is_none());

        let tx = store.transaction().unwrap();
        tx.repos().upsert(&test_repo()).unwrap();
        tx.files().upsert(&test_file("src/lib.rs")).unwrap();
        tx.commit().unwrap();

        assert!(store.repos().get("integration-repo").unwrap().is_some());
        assert!(store
            .files()
            .get("integration-repo", "src/lib.rs")
            .unwrap()
            .is_some());
    }
}

// ---------------------------------------------------------------------------
// Validation enforcement
// ---------------------------------------------------------------------------

#[test]
fn upsert_rejects_invalid_repo_record() {
    let store = MetadataStore::open_in_memory().unwrap();
    let mut repo = test_repo();
    repo.repo_id = "".to_string();

    let err = store.repos().upsert(&repo).unwrap_err();
    assert!(err.to_string().contains("validation"), "{err}");
}

#[test]
fn upsert_rejects_invalid_symbol_record() {
    let store = MetadataStore::open_in_memory().unwrap();
    store.repos().upsert(&test_repo()).unwrap();
    store.files().upsert(&test_file("src/main.rs")).unwrap();

    let mut sym = test_symbol("src/main.rs", "main", SymbolKind::Function);
    sym.confidence_score = 2.0; // out of range

    let err = store.symbols().upsert(&sym).unwrap_err();
    assert!(err.to_string().contains("validation"), "{err}");
}

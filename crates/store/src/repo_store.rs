//! Repository record CRUD operations.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use core_model::{FreshnessStatus, IndexingStatus, RepoRecord, Validate};

use crate::StoreError;

/// Accessor for repository metadata operations.
pub struct RepoStore<'a> {
    conn: &'a Connection,
}

impl<'a> RepoStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Inserts or replaces a repository record.
    ///
    /// The record is validated against the canonical [`Validate`] contract
    /// before persistence. Returns [`StoreError::Validation`] on failure.
    pub fn upsert(&self, record: &RepoRecord) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        let language_counts_json = serde_json::to_string(&record.language_counts)
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        self.conn.execute(
            "INSERT OR REPLACE INTO repos
                (repo_id, display_name, source_root, indexed_at, index_version,
                 language_counts, file_count, symbol_count, git_head,
                 registered_at, indexing_status, freshness_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.repo_id,
                record.display_name,
                record.source_root,
                record.indexed_at,
                record.index_version,
                language_counts_json,
                record.file_count,
                record.symbol_count,
                record.git_head,
                record.registered_at,
                record.indexing_status.as_str(),
                record.freshness_status.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Retrieves a repository record by ID.
    pub fn get(&self, repo_id: &str) -> Result<Option<RepoRecord>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT repo_id, display_name, source_root, indexed_at, index_version,
                    language_counts, file_count, symbol_count, git_head,
                    registered_at, indexing_status, freshness_status
             FROM repos WHERE repo_id = ?1",
        )?;

        let result = stmt
            .query_row(params![repo_id], |row| {
                let language_counts_json: String = row.get(5)?;
                let language_counts: BTreeMap<String, u64> =
                    serde_json::from_str(&language_counts_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                let indexing_status_str: String = row.get(10)?;
                let indexing_status: IndexingStatus =
                    indexing_status_str.parse().unwrap_or_default();
                let freshness_status_str: String = row.get(11)?;
                let freshness_status: FreshnessStatus =
                    freshness_status_str.parse().unwrap_or_default();

                Ok(RepoRecord {
                    repo_id: row.get(0)?,
                    display_name: row.get(1)?,
                    source_root: row.get(2)?,
                    indexed_at: row.get(3)?,
                    index_version: row.get(4)?,
                    language_counts,
                    file_count: row.get::<_, i64>(6)? as u64,
                    symbol_count: row.get::<_, i64>(7)? as u64,
                    git_head: row.get(8)?,
                    registered_at: row.get(9)?,
                    indexing_status,
                    freshness_status,
                })
            })
            .optional()?;

        Ok(result)
    }

    /// Deletes a repository and all associated files/symbols (cascading).
    pub fn delete(&self, repo_id: &str) -> Result<bool, StoreError> {
        let changed = self
            .conn
            .execute("DELETE FROM repos WHERE repo_id = ?1", params![repo_id])?;
        Ok(changed > 0)
    }

    /// Ensures a repo record exists and updates its non-aggregate metadata.
    ///
    /// Uses `INSERT OR IGNORE` followed by `UPDATE` so that existing child
    /// rows (files, symbols) are never cascade-deleted. On a fresh repo the
    /// INSERT creates the row; on a re-index the IGNORE is a no-op and the
    /// UPDATE refreshes the metadata fields.
    pub fn ensure_and_update(&self, record: &RepoRecord) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        let language_counts_json = serde_json::to_string(&record.language_counts)
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        // Create row if it doesn't exist yet. Sets registered_at on first
        // creation; the UPDATE below intentionally does not touch it.
        self.conn.execute(
            "INSERT OR IGNORE INTO repos
                (repo_id, display_name, source_root, indexed_at, index_version,
                 language_counts, file_count, symbol_count, git_head,
                 registered_at, indexing_status, freshness_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.repo_id,
                record.display_name,
                record.source_root,
                record.indexed_at,
                record.index_version,
                language_counts_json,
                record.file_count,
                record.symbol_count,
                record.git_head,
                record.registered_at,
                record.indexing_status.as_str(),
                record.freshness_status.as_str(),
            ],
        )?;

        // Update non-aggregate fields on the existing row. Note:
        // registered_at is intentionally NOT updated here — it records the
        // original registration time.
        self.conn.execute(
            "UPDATE repos SET display_name = ?2, source_root = ?3,
                 indexed_at = ?4, index_version = ?5, git_head = ?6,
                 indexing_status = ?7, freshness_status = ?8
             WHERE repo_id = ?1",
            params![
                record.repo_id,
                record.display_name,
                record.source_root,
                record.indexed_at,
                record.index_version,
                record.git_head,
                record.indexing_status.as_str(),
                record.freshness_status.as_str(),
            ],
        )?;

        Ok(())
    }

    /// Updates only the aggregate fields (`file_count`, `symbol_count`,
    /// `language_counts`) on an existing repo record. Unlike `upsert`, this
    /// uses a plain `UPDATE` so it does not trigger `ON DELETE CASCADE`.
    pub fn update_aggregates(
        &self,
        repo_id: &str,
        file_count: u64,
        symbol_count: u64,
        language_counts: &std::collections::BTreeMap<String, u64>,
    ) -> Result<(), StoreError> {
        let language_counts_json = serde_json::to_string(language_counts)
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        let changed = self.conn.execute(
            "UPDATE repos SET file_count = ?2, symbol_count = ?3, language_counts = ?4
             WHERE repo_id = ?1",
            params![repo_id, file_count, symbol_count, language_counts_json],
        )?;

        if changed == 0 {
            return Err(StoreError::Validation(format!(
                "repo '{repo_id}' not found for aggregate update"
            )));
        }
        Ok(())
    }

    /// Lists all repository IDs.
    pub fn list_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT repo_id FROM repos ORDER BY repo_id")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }
}

/// Extension trait to make `query_row` return `Option` on no rows.
trait OptionalRow<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalRow<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetadataStore;

    fn test_repo() -> RepoRecord {
        let mut language_counts = BTreeMap::new();
        language_counts.insert("rust".to_string(), 10);
        language_counts.insert("typescript".to_string(), 5);

        RepoRecord {
            repo_id: "test-repo".to_string(),
            display_name: "Test Repository".to_string(),
            source_root: "/home/user/repos/test".to_string(),
            indexed_at: "2025-01-15T10:30:00Z".to_string(),
            index_version: "1.0.0".to_string(),
            language_counts,
            file_count: 15,
            symbol_count: 150,
            git_head: Some("abc123def456".to_string()),
            registered_at: Some("2025-01-15T10:30:00Z".to_string()),
            indexing_status: IndexingStatus::Ready,
            freshness_status: FreshnessStatus::Fresh,
        }
    }

    #[test]
    fn upsert_and_get_round_trips() {
        let store = MetadataStore::open_in_memory().unwrap();
        let repo = test_repo();

        store.repos().upsert(&repo).unwrap();
        let loaded = store.repos().get("test-repo").unwrap().unwrap();

        assert_eq!(loaded.repo_id, repo.repo_id);
        assert_eq!(loaded.display_name, repo.display_name);
        assert_eq!(loaded.source_root, repo.source_root);
        assert_eq!(loaded.indexed_at, repo.indexed_at);
        assert_eq!(loaded.index_version, repo.index_version);
        assert_eq!(loaded.language_counts, repo.language_counts);
        assert_eq!(loaded.file_count, repo.file_count);
        assert_eq!(loaded.symbol_count, repo.symbol_count);
        assert_eq!(loaded.git_head, repo.git_head);
        assert_eq!(loaded.registered_at, repo.registered_at);
        assert_eq!(loaded.indexing_status, repo.indexing_status);
        assert_eq!(loaded.freshness_status, repo.freshness_status);
    }

    #[test]
    fn get_returns_none_for_missing() {
        let store = MetadataStore::open_in_memory().unwrap();
        assert!(store.repos().get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_existing() {
        let store = MetadataStore::open_in_memory().unwrap();
        let mut repo = test_repo();
        store.repos().upsert(&repo).unwrap();

        repo.file_count = 99;
        repo.git_head = Some("new-head".to_string());
        store.repos().upsert(&repo).unwrap();

        let loaded = store.repos().get("test-repo").unwrap().unwrap();
        assert_eq!(loaded.file_count, 99);
        assert_eq!(loaded.git_head, Some("new-head".to_string()));
    }

    #[test]
    fn delete_removes_repo() {
        let store = MetadataStore::open_in_memory().unwrap();
        store.repos().upsert(&test_repo()).unwrap();

        assert!(store.repos().delete("test-repo").unwrap());
        assert!(store.repos().get("test-repo").unwrap().is_none());
    }

    #[test]
    fn delete_returns_false_for_missing() {
        let store = MetadataStore::open_in_memory().unwrap();
        assert!(!store.repos().delete("nonexistent").unwrap());
    }

    #[test]
    fn list_ids_returns_sorted() {
        let store = MetadataStore::open_in_memory().unwrap();
        let mut r1 = test_repo();
        r1.repo_id = "beta-repo".to_string();
        let mut r2 = test_repo();
        r2.repo_id = "alpha-repo".to_string();

        store.repos().upsert(&r1).unwrap();
        store.repos().upsert(&r2).unwrap();

        let ids = store.repos().list_ids().unwrap();
        assert_eq!(ids, vec!["alpha-repo", "beta-repo"]);
    }

    #[test]
    fn git_head_none_round_trips() {
        let store = MetadataStore::open_in_memory().unwrap();
        let mut repo = test_repo();
        repo.git_head = None;
        store.repos().upsert(&repo).unwrap();

        let loaded = store.repos().get("test-repo").unwrap().unwrap();
        assert_eq!(loaded.git_head, None);
    }

    #[test]
    fn upsert_rejects_invalid_record() {
        let store = MetadataStore::open_in_memory().unwrap();
        let mut repo = test_repo();
        repo.repo_id = "".to_string(); // fails validation

        let err = store.repos().upsert(&repo).unwrap_err();
        assert!(err.to_string().contains("validation"), "{err}");
    }

    #[test]
    fn update_aggregates_modifies_counts_without_cascade() {
        let store = MetadataStore::open_in_memory().unwrap();
        store.repos().upsert(&test_repo()).unwrap();

        // Insert a file to verify cascade is NOT triggered.
        store
            .files()
            .upsert(&core_model::FileRecord {
                repo_id: "test-repo".to_string(),
                file_path: "src/main.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "sha256:abc".to_string(),
                summary: "test".to_string(),
                symbol_count: 5,
                quality_mix: core_model::QualityMix {
                    semantic_percent: 0.0,
                    syntax_percent: 100.0,
                },
                updated_at: "2025-01-15T10:30:00Z".to_string(),
            })
            .unwrap();

        let mut new_counts = BTreeMap::new();
        new_counts.insert("rust".to_string(), 3);
        store
            .repos()
            .update_aggregates("test-repo", 3, 42, &new_counts)
            .unwrap();

        let loaded = store.repos().get("test-repo").unwrap().unwrap();
        assert_eq!(loaded.file_count, 3);
        assert_eq!(loaded.symbol_count, 42);
        assert_eq!(loaded.language_counts, new_counts);

        // File should still exist (no cascade).
        assert!(store
            .files()
            .get("test-repo", "src/main.rs")
            .unwrap()
            .is_some());
    }

    #[test]
    fn update_aggregates_fails_for_missing_repo() {
        let store = MetadataStore::open_in_memory().unwrap();
        let err = store
            .repos()
            .update_aggregates("nonexistent", 0, 0, &BTreeMap::new())
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }
}

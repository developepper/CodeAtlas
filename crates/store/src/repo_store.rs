//! Repository record CRUD operations.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use core_model::{RepoRecord, Validate};

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
                 language_counts, file_count, symbol_count, git_head)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
            ],
        )?;
        Ok(())
    }

    /// Retrieves a repository record by ID.
    pub fn get(&self, repo_id: &str) -> Result<Option<RepoRecord>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT repo_id, display_name, source_root, indexed_at, index_version,
                    language_counts, file_count, symbol_count, git_head
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
}

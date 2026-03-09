//! File record CRUD operations.

use rusqlite::{params, Connection};

use core_model::{FileRecord, QualityMix, Validate};

use crate::StoreError;

/// Accessor for file metadata operations.
pub struct FileStore<'a> {
    conn: &'a Connection,
}

impl<'a> FileStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Inserts or replaces a file record.
    ///
    /// The record is validated against the canonical [`Validate`] contract
    /// before persistence. Returns [`StoreError::Validation`] on failure.
    pub fn upsert(&self, record: &FileRecord) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
                (repo_id, file_path, language, file_hash, summary,
                 symbol_count, semantic_pct, syntax_pct, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.repo_id,
                record.file_path,
                record.language,
                record.file_hash,
                record.summary,
                record.symbol_count,
                record.quality_mix.semantic_percent,
                record.quality_mix.syntax_percent,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieves a file record by repo ID and file path.
    pub fn get(&self, repo_id: &str, file_path: &str) -> Result<Option<FileRecord>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT repo_id, file_path, language, file_hash, summary,
                    symbol_count, semantic_pct, syntax_pct, updated_at
             FROM files WHERE repo_id = ?1 AND file_path = ?2",
        )?;

        let result = stmt
            .query_row(params![repo_id, file_path], |row| {
                Ok(FileRecord {
                    repo_id: row.get(0)?,
                    file_path: row.get(1)?,
                    language: row.get(2)?,
                    file_hash: row.get(3)?,
                    summary: row.get(4)?,
                    symbol_count: row.get::<_, i64>(5)? as u64,
                    quality_mix: QualityMix {
                        semantic_percent: row.get(6)?,
                        syntax_percent: row.get(7)?,
                    },
                    updated_at: row.get(8)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    /// Deletes a file record.
    pub fn delete(&self, repo_id: &str, file_path: &str) -> Result<bool, StoreError> {
        let changed = self.conn.execute(
            "DELETE FROM files WHERE repo_id = ?1 AND file_path = ?2",
            params![repo_id, file_path],
        )?;
        Ok(changed > 0)
    }

    /// Lists all file paths for a repository, sorted.
    pub fn list_paths(&self, repo_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_path FROM files WHERE repo_id = ?1 ORDER BY file_path")?;
        let paths = stmt
            .query_map(params![repo_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(paths)
    }

    /// Deletes all file records for a repository whose paths are **not** in
    /// `keep_paths`. Cascading foreign keys remove associated symbols.
    ///
    /// Returns the number of file records deleted.
    pub fn delete_except(&self, repo_id: &str, keep_paths: &[&str]) -> Result<u64, StoreError> {
        if keep_paths.is_empty() {
            let changed = self
                .conn
                .execute("DELETE FROM files WHERE repo_id = ?1", params![repo_id])?;
            return Ok(changed as u64);
        }

        // Build a parameterised IN-list. SQLite limits are generous (32766
        // for recent versions), but this is fine for typical repos.
        let placeholders: String = keep_paths
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");

        let sql =
            format!("DELETE FROM files WHERE repo_id = ?1 AND file_path NOT IN ({placeholders})");

        let mut stmt = self.conn.prepare(&sql)?;

        // Bind repo_id at index 1, then each keep_path starting at index 2.
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            Vec::with_capacity(1 + keep_paths.len());
        param_values.push(Box::new(repo_id.to_string()));
        for path in keep_paths {
            param_values.push(Box::new(path.to_string()));
        }

        let refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let changed = stmt.execute(refs.as_slice())?;
        Ok(changed as u64)
    }

    /// Returns the total number of file records for a repository.
    pub fn count(&self, repo_id: &str) -> Result<u64, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE repo_id = ?1",
            params![repo_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Returns per-language file counts for a repository.
    pub fn aggregate_language_counts(
        &self,
        repo_id: &str,
    ) -> Result<std::collections::BTreeMap<String, u64>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT language, COUNT(*) FROM files WHERE repo_id = ?1 GROUP BY language ORDER BY language",
        )?;
        let mut counts = std::collections::BTreeMap::new();
        let rows = stmt.query_map(params![repo_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (lang, count) = row?;
            counts.insert(lang, count as u64);
        }
        Ok(counts)
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
    use std::collections::BTreeMap;

    fn setup_store_with_repo() -> MetadataStore {
        let store = MetadataStore::open_in_memory().unwrap();
        store
            .repos()
            .upsert(&core_model::RepoRecord {
                repo_id: "test-repo".to_string(),
                display_name: "Test".to_string(),
                source_root: "/tmp/test".to_string(),
                indexed_at: "2025-01-15T10:30:00Z".to_string(),
                index_version: "1.0.0".to_string(),
                language_counts: BTreeMap::new(),
                file_count: 0,
                symbol_count: 0,
                git_head: None,
            })
            .unwrap();
        store
    }

    fn test_file() -> FileRecord {
        FileRecord {
            repo_id: "test-repo".to_string(),
            file_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            file_hash: "sha256:abc123".to_string(),
            summary: "Main entry point".to_string(),
            symbol_count: 5,
            quality_mix: QualityMix {
                semantic_percent: 0.0,
                syntax_percent: 100.0,
            },
            updated_at: "2025-01-15T10:30:00Z".to_string(),
        }
    }

    #[test]
    fn upsert_and_get_round_trips() {
        let store = setup_store_with_repo();
        let file = test_file();

        store.files().upsert(&file).unwrap();
        let loaded = store
            .files()
            .get("test-repo", "src/main.rs")
            .unwrap()
            .unwrap();

        assert_eq!(loaded.repo_id, file.repo_id);
        assert_eq!(loaded.file_path, file.file_path);
        assert_eq!(loaded.language, file.language);
        assert_eq!(loaded.file_hash, file.file_hash);
        assert_eq!(loaded.summary, file.summary);
        assert_eq!(loaded.symbol_count, file.symbol_count);
        assert!(
            (loaded.quality_mix.semantic_percent - file.quality_mix.semantic_percent).abs()
                < f32::EPSILON
        );
        assert!(
            (loaded.quality_mix.syntax_percent - file.quality_mix.syntax_percent).abs()
                < f32::EPSILON
        );
        assert_eq!(loaded.updated_at, file.updated_at);
    }

    #[test]
    fn get_returns_none_for_missing() {
        let store = setup_store_with_repo();
        assert!(store.files().get("test-repo", "nope.rs").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_existing() {
        let store = setup_store_with_repo();
        let mut file = test_file();
        store.files().upsert(&file).unwrap();

        file.file_hash = "sha256:updated".to_string();
        file.symbol_count = 10;
        store.files().upsert(&file).unwrap();

        let loaded = store
            .files()
            .get("test-repo", "src/main.rs")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.file_hash, "sha256:updated");
        assert_eq!(loaded.symbol_count, 10);
    }

    #[test]
    fn delete_removes_file() {
        let store = setup_store_with_repo();
        store.files().upsert(&test_file()).unwrap();

        assert!(store.files().delete("test-repo", "src/main.rs").unwrap());
        assert!(store
            .files()
            .get("test-repo", "src/main.rs")
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_paths_returns_sorted() {
        let store = setup_store_with_repo();
        let mut f1 = test_file();
        f1.file_path = "src/lib.rs".to_string();
        let mut f2 = test_file();
        f2.file_path = "src/app.rs".to_string();

        store.files().upsert(&f1).unwrap();
        store.files().upsert(&f2).unwrap();

        let paths = store.files().list_paths("test-repo").unwrap();
        assert_eq!(paths, vec!["src/app.rs", "src/lib.rs"]);
    }

    #[test]
    fn cascade_delete_removes_files_with_repo() {
        let store = setup_store_with_repo();
        store.files().upsert(&test_file()).unwrap();

        store.repos().delete("test-repo").unwrap();
        assert!(store
            .files()
            .get("test-repo", "src/main.rs")
            .unwrap()
            .is_none());
    }

    #[test]
    fn upsert_rejects_invalid_record() {
        let store = setup_store_with_repo();
        let mut file = test_file();
        file.file_hash = "".to_string(); // fails validation

        let err = store.files().upsert(&file).unwrap_err();
        assert!(err.to_string().contains("validation"), "{err}");
    }

    #[test]
    fn delete_except_removes_stale_files() {
        let store = setup_store_with_repo();
        let mut f1 = test_file();
        f1.file_path = "src/main.rs".to_string();
        let mut f2 = test_file();
        f2.file_path = "src/lib.rs".to_string();
        let mut f3 = test_file();
        f3.file_path = "src/old.rs".to_string();

        store.files().upsert(&f1).unwrap();
        store.files().upsert(&f2).unwrap();
        store.files().upsert(&f3).unwrap();

        // Keep only main.rs and lib.rs — old.rs should be deleted.
        let deleted = store
            .files()
            .delete_except("test-repo", &["src/main.rs", "src/lib.rs"])
            .unwrap();
        assert_eq!(deleted, 1);

        let paths = store.files().list_paths("test-repo").unwrap();
        assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn delete_except_with_empty_keep_removes_all() {
        let store = setup_store_with_repo();
        store.files().upsert(&test_file()).unwrap();

        let deleted = store.files().delete_except("test-repo", &[]).unwrap();
        assert_eq!(deleted, 1);
        assert!(store.files().list_paths("test-repo").unwrap().is_empty());
    }

    #[test]
    fn delete_except_returns_zero_when_all_kept() {
        let store = setup_store_with_repo();
        store.files().upsert(&test_file()).unwrap();

        let deleted = store
            .files()
            .delete_except("test-repo", &["src/main.rs"])
            .unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn count_returns_file_count() {
        let store = setup_store_with_repo();
        assert_eq!(store.files().count("test-repo").unwrap(), 0);

        store.files().upsert(&test_file()).unwrap();
        assert_eq!(store.files().count("test-repo").unwrap(), 1);

        let mut f2 = test_file();
        f2.file_path = "src/lib.rs".to_string();
        store.files().upsert(&f2).unwrap();
        assert_eq!(store.files().count("test-repo").unwrap(), 2);
    }

    #[test]
    fn aggregate_language_counts_groups_by_language() {
        let store = setup_store_with_repo();

        let mut f1 = test_file();
        f1.file_path = "src/main.rs".to_string();
        f1.language = "rust".to_string();
        let mut f2 = test_file();
        f2.file_path = "src/lib.rs".to_string();
        f2.language = "rust".to_string();
        let mut f3 = test_file();
        f3.file_path = "app.ts".to_string();
        f3.language = "typescript".to_string();

        store.files().upsert(&f1).unwrap();
        store.files().upsert(&f2).unwrap();
        store.files().upsert(&f3).unwrap();

        let counts = store
            .files()
            .aggregate_language_counts("test-repo")
            .unwrap();
        assert_eq!(counts.get("rust"), Some(&2));
        assert_eq!(counts.get("typescript"), Some(&1));
        assert_eq!(counts.len(), 2);
    }
}

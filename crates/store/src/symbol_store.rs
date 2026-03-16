//! Symbol record CRUD operations.

use rusqlite::{params, Connection};

use core_model::{CapabilityTier, SymbolKind, SymbolRecord, Validate};

use crate::StoreError;

/// Accessor for symbol metadata operations.
pub struct SymbolStore<'a> {
    conn: &'a Connection,
}

impl<'a> SymbolStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Inserts or replaces a symbol record.
    ///
    /// The record is validated against the canonical [`Validate`] contract
    /// before persistence. Returns [`StoreError::Validation`] on failure.
    pub fn upsert(&self, record: &SymbolRecord) -> Result<(), StoreError> {
        record
            .validate()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        let keywords_json = record
            .keywords
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Validation(e.to_string()))?;
        let decorators_json = record
            .decorators_or_attributes
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Validation(e.to_string()))?;
        let refs_json = record
            .semantic_refs
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        let modifiers_json = record
            .modifiers
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Validation(e.to_string()))?;

        self.conn.execute(
            "INSERT OR REPLACE INTO symbols
                (id, repo_id, file_path, language, kind, name, qualified_name,
                 signature, start_line, end_line, start_byte, byte_length,
                 content_hash, capability_tier, confidence_score, source_backend,
                 indexed_at, docstring, summary, parent_symbol_id,
                 keywords, decorators_or_attributes, semantic_refs,
                 container_symbol_id, namespace_path, raw_kind, modifiers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
                     ?24, ?25, ?26, ?27)",
            params![
                record.id,
                record.repo_id,
                record.file_path,
                record.language,
                record.kind.as_str(),
                record.name,
                record.qualified_name,
                record.signature,
                record.start_line,
                record.end_line,
                record.start_byte,
                record.byte_length,
                record.content_hash,
                record.capability_tier.as_str(),
                record.confidence_score,
                record.source_backend,
                record.indexed_at,
                record.docstring,
                record.summary,
                record.parent_symbol_id,
                keywords_json,
                decorators_json,
                refs_json,
                record.container_symbol_id,
                record.namespace_path,
                record.raw_kind,
                modifiers_json,
            ],
        )?;
        Ok(())
    }

    /// Retrieves a symbol record by its globally unique ID.
    ///
    /// Symbol IDs include the repo_id prefix (`{repo_id}//{file}::{name}#{kind}`)
    /// so they are unique across repos in a shared store.
    pub fn get(&self, id: &str) -> Result<Option<SymbolRecord>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, repo_id, file_path, language, kind, name, qualified_name,
                    signature, start_line, end_line, start_byte, byte_length,
                    content_hash, capability_tier, confidence_score, source_backend,
                    indexed_at, docstring, summary, parent_symbol_id,
                    keywords, decorators_or_attributes, semantic_refs,
                    container_symbol_id, namespace_path, raw_kind, modifiers
             FROM symbols WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| {
                let kind_str: String = row.get(4)?;
                let tier_str: String = row.get(13)?;
                let keywords_json: Option<String> = row.get(20)?;
                let decorators_json: Option<String> = row.get(21)?;
                let refs_json: Option<String> = row.get(22)?;

                Ok(SymbolRecord {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    file_path: row.get(2)?,
                    language: row.get(3)?,
                    kind: parse_symbol_kind(&kind_str),
                    name: row.get(5)?,
                    qualified_name: row.get(6)?,
                    signature: row.get(7)?,
                    start_line: row.get::<_, i32>(8)? as u32,
                    end_line: row.get::<_, i32>(9)? as u32,
                    start_byte: row.get::<_, i64>(10)? as u64,
                    byte_length: row.get::<_, i64>(11)? as u64,
                    content_hash: row.get(12)?,
                    capability_tier: parse_capability_tier(&tier_str),
                    confidence_score: row.get(14)?,
                    source_backend: row.get(15)?,
                    indexed_at: row.get(16)?,
                    docstring: row.get(17)?,
                    summary: row.get(18)?,
                    parent_symbol_id: row.get(19)?,
                    keywords: keywords_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(20, e)))
                        .transpose()?,
                    decorators_or_attributes: decorators_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(21, e)))
                        .transpose()?,
                    semantic_refs: refs_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(22, e)))
                        .transpose()?,
                    container_symbol_id: row.get(23)?,
                    namespace_path: row.get(24)?,
                    raw_kind: row.get(25)?,
                    modifiers: {
                        let json: Option<String> = row.get(26)?;
                        json.map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(26, e)))
                            .transpose()?
                    },
                })
            })
            .optional()?;

        Ok(result)
    }

    /// Deletes a symbol record by its globally unique ID.
    pub fn delete(&self, id: &str) -> Result<bool, StoreError> {
        let changed = self
            .conn
            .execute("DELETE FROM symbols WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// Lists all symbol IDs for a given repo and file path, sorted.
    pub fn list_ids_for_file(
        &self,
        repo_id: &str,
        file_path: &str,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM symbols WHERE repo_id = ?1 AND file_path = ?2 ORDER BY id")?;
        let ids = stmt
            .query_map(params![repo_id, file_path], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    /// Deletes all symbols for a given repo and file path.
    pub fn delete_for_file(&self, repo_id: &str, file_path: &str) -> Result<u64, StoreError> {
        let changed = self.conn.execute(
            "DELETE FROM symbols WHERE repo_id = ?1 AND file_path = ?2",
            params![repo_id, file_path],
        )?;
        Ok(changed as u64)
    }

    /// Retrieves candidate symbols for a repository, filtered by optional criteria.
    ///
    /// Results are returned in deterministic order (by `id`). Ranking is the
    /// caller's responsibility.
    pub fn search_candidates(
        &self,
        repo_id: &str,
        kind: Option<SymbolKind>,
        language: Option<&str>,
        capability_tier: Option<CapabilityTier>,
        file_path: Option<&str>,
    ) -> Result<Vec<SymbolRecord>, StoreError> {
        let mut sql = String::from(
            "SELECT id, repo_id, file_path, language, kind, name, qualified_name,
                    signature, start_line, end_line, start_byte, byte_length,
                    content_hash, capability_tier, confidence_score, source_backend,
                    indexed_at, docstring, summary, parent_symbol_id,
                    keywords, decorators_or_attributes, semantic_refs,
                    container_symbol_id, namespace_path, raw_kind, modifiers
             FROM symbols WHERE repo_id = ?1",
        );

        let mut param_index = 2u32;
        let mut param_strings: Vec<String> = Vec::new();

        if let Some(k) = kind {
            sql.push_str(&format!(" AND kind = ?{param_index}"));
            param_strings.push(k.as_str().to_string());
            param_index += 1;
        }
        if let Some(lang) = language {
            sql.push_str(&format!(" AND language = ?{param_index}"));
            param_strings.push(lang.to_string());
            param_index += 1;
        }
        if let Some(tier) = capability_tier {
            sql.push_str(&format!(" AND capability_tier = ?{param_index}"));
            param_strings.push(tier.as_str().to_string());
            param_index += 1;
        }
        if let Some(fp) = file_path {
            sql.push_str(&format!(" AND file_path = ?{param_index}"));
            param_strings.push(fp.to_string());
        }

        sql.push_str(" ORDER BY id");

        let mut stmt = self.conn.prepare(&sql)?;

        // Build parameter slice: repo_id + any dynamic filters.
        let mut params_vec: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
        let repo_id_owned = repo_id.to_string();
        params_vec.push(&repo_id_owned);
        for p in &param_strings {
            params_vec.push(p);
        }

        let rows = stmt
            .query_map(params_vec.as_slice(), |row| {
                let kind_str: String = row.get(4)?;
                let tier_str: String = row.get(13)?;
                let keywords_json: Option<String> = row.get(20)?;
                let decorators_json: Option<String> = row.get(21)?;
                let refs_json: Option<String> = row.get(22)?;

                Ok(SymbolRecord {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    file_path: row.get(2)?,
                    language: row.get(3)?,
                    kind: parse_symbol_kind(&kind_str),
                    name: row.get(5)?,
                    qualified_name: row.get(6)?,
                    signature: row.get(7)?,
                    start_line: row.get::<_, i32>(8)? as u32,
                    end_line: row.get::<_, i32>(9)? as u32,
                    start_byte: row.get::<_, i64>(10)? as u64,
                    byte_length: row.get::<_, i64>(11)? as u64,
                    content_hash: row.get(12)?,
                    capability_tier: parse_capability_tier(&tier_str),
                    confidence_score: row.get(14)?,
                    source_backend: row.get(15)?,
                    indexed_at: row.get(16)?,
                    docstring: row.get(17)?,
                    summary: row.get(18)?,
                    parent_symbol_id: row.get(19)?,
                    keywords: keywords_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(20, e)))
                        .transpose()?,
                    decorators_or_attributes: decorators_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(21, e)))
                        .transpose()?,
                    semantic_refs: refs_json
                        .map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(22, e)))
                        .transpose()?,
                    container_symbol_id: row.get(23)?,
                    namespace_path: row.get(24)?,
                    raw_kind: row.get(25)?,
                    modifiers: {
                        let json: Option<String> = row.get(26)?;
                        json.map(|j| serde_json::from_str(&j).map_err(|e| json_read_err(26, e)))
                            .transpose()?
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Searches symbols via FTS5 full-text index, filtered by repo.
    ///
    /// Returns `(symbol_id, fts_rank)` pairs ordered by relevance then ID for
    /// deterministic tie-breaking. The caller joins with full symbol records.
    ///
    /// The raw query string is normalized to plain FTS5 terms (implicit AND)
    /// before being passed to MATCH, stripping any FTS5 special syntax.
    pub fn search_text_fts(
        &self,
        repo_id: &str,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<(String, f64)>, usize), StoreError> {
        let normalized = normalize_fts_query(query);
        if normalized.is_empty() {
            return Ok((Vec::new(), 0));
        }

        // Count total matches first.
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts
             JOIN symbols ON symbols.id = symbols_fts.id
             WHERE symbols_fts MATCH ?1 AND symbols.repo_id = ?2",
            params![&normalized, repo_id],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT symbols_fts.id, rank
             FROM symbols_fts
             JOIN symbols ON symbols.id = symbols_fts.id
             WHERE symbols_fts MATCH ?1 AND symbols.repo_id = ?2
             ORDER BY rank, symbols_fts.id
             LIMIT ?3 OFFSET ?4",
        )?;

        let rows = stmt
            .query_map(
                params![&normalized, repo_id, limit as i64, offset as i64],
                |row| {
                    let id: String = row.get(0)?;
                    let rank: f64 = row.get(1)?;
                    Ok((id, rank))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((rows, total as usize))
    }

    /// Returns the total number of symbol records for a repository.
    pub fn count_for_repo(&self, repo_id: &str) -> Result<u64, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE repo_id = ?1",
            params![repo_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_capability_tier(s: &str) -> CapabilityTier {
    s.parse().unwrap_or(CapabilityTier::FileOnly)
}

fn parse_symbol_kind(s: &str) -> SymbolKind {
    SymbolKind::from_id_token(s).unwrap_or(SymbolKind::Unknown)
}

fn json_read_err(col: usize, e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
}

/// FTS5 reserved operator keywords that must be lowercased to be treated
/// as plain search terms rather than query syntax.
const FTS5_OPERATORS: &[&str] = &["AND", "OR", "NOT", "NEAR"];

/// Normalizes raw user input into a safe FTS5 query string.
///
/// Strips special characters, lowercases FTS5 reserved operators so they are
/// treated as plain terms, and joins with spaces (implicit AND in FTS5).
/// Returns an empty string if no valid tokens remain.
fn normalize_fts_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if FTS5_OPERATORS.contains(&cleaned.as_str()) {
                cleaned.to_lowercase()
            } else {
                cleaned
            }
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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
    use core_model::{CapabilityTier, FileRecord, FreshnessStatus, IndexingStatus, RepoRecord};
    use std::collections::BTreeMap;

    fn setup_store() -> MetadataStore {
        let store = MetadataStore::open_in_memory().unwrap();
        store
            .repos()
            .upsert(&RepoRecord {
                repo_id: "test-repo".to_string(),
                display_name: "Test".to_string(),
                source_root: "/tmp/test".to_string(),
                indexed_at: "2025-01-15T10:30:00Z".to_string(),
                index_version: "1.0.0".to_string(),
                language_counts: BTreeMap::new(),
                file_count: 0,
                symbol_count: 0,
                git_head: None,
                registered_at: Some("2025-01-15T10:30:00Z".to_string()),
                indexing_status: IndexingStatus::Ready,
                freshness_status: FreshnessStatus::Fresh,
            })
            .unwrap();
        store
            .files()
            .upsert(&FileRecord {
                repo_id: "test-repo".to_string(),
                file_path: "src/main.rs".to_string(),
                language: "rust".to_string(),
                file_hash: "sha256:abc".to_string(),
                summary: "test file".to_string(),
                symbol_count: 0,
                capability_tier: CapabilityTier::SyntaxOnly,
                updated_at: "2025-01-15T10:30:00Z".to_string(),
            })
            .unwrap();
        store
    }

    fn test_symbol() -> SymbolRecord {
        SymbolRecord {
            id: "test-repo//src/main.rs::main#function".to_string(),
            repo_id: "test-repo".to_string(),
            file_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            kind: SymbolKind::Function,
            name: "main".to_string(),
            qualified_name: "main".to_string(),
            signature: "fn main()".to_string(),
            start_line: 1,
            end_line: 3,
            start_byte: 0,
            byte_length: 25,
            content_hash: "sha256:def456".to_string(),
            capability_tier: CapabilityTier::SyntaxOnly,
            confidence_score: 0.7,
            source_backend: "syntax-treesitter-rust".to_string(),
            indexed_at: "2025-01-15T10:30:00Z".to_string(),
            docstring: Some("Entry point".to_string()),
            summary: None,
            parent_symbol_id: None,
            keywords: Some(vec!["main".to_string(), "entry".to_string()]),
            decorators_or_attributes: None,
            semantic_refs: None,
            container_symbol_id: None,
            namespace_path: None,
            raw_kind: None,
            modifiers: None,
        }
    }

    #[test]
    fn upsert_and_get_round_trips() {
        let store = setup_store();
        let sym = test_symbol();

        store.symbols().upsert(&sym).unwrap();
        let loaded = store
            .symbols()
            .get("test-repo//src/main.rs::main#function")
            .unwrap()
            .unwrap();

        assert_eq!(loaded.id, sym.id);
        assert_eq!(loaded.repo_id, sym.repo_id);
        assert_eq!(loaded.file_path, sym.file_path);
        assert_eq!(loaded.language, sym.language);
        assert_eq!(loaded.kind, sym.kind);
        assert_eq!(loaded.name, sym.name);
        assert_eq!(loaded.qualified_name, sym.qualified_name);
        assert_eq!(loaded.signature, sym.signature);
        assert_eq!(loaded.start_line, sym.start_line);
        assert_eq!(loaded.end_line, sym.end_line);
        assert_eq!(loaded.start_byte, sym.start_byte);
        assert_eq!(loaded.byte_length, sym.byte_length);
        assert_eq!(loaded.content_hash, sym.content_hash);
        assert_eq!(loaded.capability_tier, sym.capability_tier);
        assert!((loaded.confidence_score - sym.confidence_score).abs() < f32::EPSILON);
        assert_eq!(loaded.source_backend, sym.source_backend);
        assert_eq!(loaded.indexed_at, sym.indexed_at);
        assert_eq!(loaded.docstring, sym.docstring);
        assert_eq!(loaded.summary, sym.summary);
        assert_eq!(loaded.parent_symbol_id, sym.parent_symbol_id);
        assert_eq!(loaded.keywords, sym.keywords);
        assert_eq!(
            loaded.decorators_or_attributes,
            sym.decorators_or_attributes
        );
        assert_eq!(loaded.semantic_refs, sym.semantic_refs);
    }

    #[test]
    fn get_returns_none_for_missing() {
        let store = setup_store();
        assert!(store.symbols().get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_existing() {
        let store = setup_store();
        let mut sym = test_symbol();
        store.symbols().upsert(&sym).unwrap();

        sym.confidence_score = 0.9;
        sym.capability_tier = CapabilityTier::SyntaxPlusSemantic;
        store.symbols().upsert(&sym).unwrap();

        let loaded = store
            .symbols()
            .get("test-repo//src/main.rs::main#function")
            .unwrap()
            .unwrap();
        assert!((loaded.confidence_score - 0.9).abs() < f32::EPSILON);
        assert_eq!(loaded.capability_tier, CapabilityTier::SyntaxPlusSemantic);
    }

    #[test]
    fn delete_removes_symbol() {
        let store = setup_store();
        store.symbols().upsert(&test_symbol()).unwrap();

        assert!(store
            .symbols()
            .delete("test-repo//src/main.rs::main#function")
            .unwrap());
        assert!(store
            .symbols()
            .get("test-repo//src/main.rs::main#function")
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_ids_for_file_returns_sorted() {
        let store = setup_store();
        let mut s1 = test_symbol();
        s1.id = "test-repo//src/main.rs::alpha#function".to_string();
        s1.name = "alpha".to_string();
        s1.qualified_name = "alpha".to_string();

        let mut s2 = test_symbol();
        s2.id = "test-repo//src/main.rs::beta#function".to_string();
        s2.name = "beta".to_string();
        s2.qualified_name = "beta".to_string();

        store.symbols().upsert(&s2).unwrap();
        store.symbols().upsert(&s1).unwrap();

        let ids = store
            .symbols()
            .list_ids_for_file("test-repo", "src/main.rs")
            .unwrap();
        assert_eq!(
            ids,
            vec![
                "test-repo//src/main.rs::alpha#function",
                "test-repo//src/main.rs::beta#function"
            ]
        );
    }

    #[test]
    fn delete_for_file_removes_all() {
        let store = setup_store();
        let s1 = test_symbol();
        let mut s2 = test_symbol();
        s2.id = "test-repo//src/main.rs::helper#function".to_string();
        s2.name = "helper".to_string();
        s2.qualified_name = "helper".to_string();

        store.symbols().upsert(&s1).unwrap();
        store.symbols().upsert(&s2).unwrap();

        let deleted = store
            .symbols()
            .delete_for_file("test-repo", "src/main.rs")
            .unwrap();
        assert_eq!(deleted, 2);

        let ids = store
            .symbols()
            .list_ids_for_file("test-repo", "src/main.rs")
            .unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn optional_fields_round_trip_as_none() {
        let store = setup_store();
        let mut sym = test_symbol();
        sym.docstring = None;
        sym.summary = None;
        sym.parent_symbol_id = None;
        sym.keywords = None;
        sym.decorators_or_attributes = None;
        sym.semantic_refs = None;

        store.symbols().upsert(&sym).unwrap();
        let loaded = store
            .symbols()
            .get("test-repo//src/main.rs::main#function")
            .unwrap()
            .unwrap();

        assert_eq!(loaded.docstring, None);
        assert_eq!(loaded.summary, None);
        assert_eq!(loaded.parent_symbol_id, None);
        assert_eq!(loaded.keywords, None);
        assert_eq!(loaded.decorators_or_attributes, None);
        assert_eq!(loaded.semantic_refs, None);
    }

    #[test]
    fn cascade_delete_removes_symbols_with_file() {
        let store = setup_store();
        store.symbols().upsert(&test_symbol()).unwrap();

        store.files().delete("test-repo", "src/main.rs").unwrap();
        assert!(store
            .symbols()
            .get("test-repo//src/main.rs::main#function")
            .unwrap()
            .is_none());
    }

    #[test]
    fn upsert_rejects_invalid_record() {
        let store = setup_store();
        let mut sym = test_symbol();
        sym.name = "".to_string(); // fails validation

        let err = store.symbols().upsert(&sym).unwrap_err();
        assert!(err.to_string().contains("validation"), "{err}");
    }

    #[test]
    fn normalize_fts_strips_special_chars() {
        assert_eq!(normalize_fts_query("\"exact phrase\""), "exact phrase");
        assert_eq!(normalize_fts_query("foo*"), "foo");
        assert_eq!(normalize_fts_query("col:name"), "colname");
    }

    #[test]
    fn normalize_fts_lowercases_reserved_operators() {
        assert_eq!(normalize_fts_query("foo AND bar"), "foo and bar");
        assert_eq!(normalize_fts_query("foo OR bar"), "foo or bar");
        assert_eq!(normalize_fts_query("NOT foo"), "not foo");
        assert_eq!(normalize_fts_query("NEAR foo"), "near foo");
        // Standalone operator becomes a plain lowercase term.
        assert_eq!(normalize_fts_query("OR"), "or");
        assert_eq!(normalize_fts_query("AND NOT"), "and not");
    }

    #[test]
    fn normalize_fts_preserves_plain_tokens() {
        assert_eq!(normalize_fts_query("parse_config"), "parse_config");
        assert_eq!(normalize_fts_query("http server"), "http server");
    }

    #[test]
    fn normalize_fts_empty_after_stripping() {
        assert_eq!(normalize_fts_query("***"), "");
        assert_eq!(normalize_fts_query("\"\""), "");
        assert_eq!(normalize_fts_query("  "), "");
    }

    #[test]
    fn count_for_repo_returns_total_symbols() {
        let store = setup_store();
        assert_eq!(store.symbols().count_for_repo("test-repo").unwrap(), 0);

        store.symbols().upsert(&test_symbol()).unwrap();
        assert_eq!(store.symbols().count_for_repo("test-repo").unwrap(), 1);

        let mut s2 = test_symbol();
        s2.id = "test-repo//src/main.rs::helper#function".to_string();
        s2.name = "helper".to_string();
        s2.qualified_name = "helper".to_string();
        store.symbols().upsert(&s2).unwrap();
        assert_eq!(store.symbols().count_for_repo("test-repo").unwrap(), 2);

        // Different repo should not be counted.
        assert_eq!(store.symbols().count_for_repo("other-repo").unwrap(), 0);
    }
}

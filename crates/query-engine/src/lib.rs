use std::fmt;

use core_model::{FileRecord, QualityLevel, RepoRecord, SymbolKind, SymbolRecord};

pub mod ranking;
pub mod store_service;

pub use store_service::StoreQueryService;

// ── Error ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum QueryError {
    EmptyQuery,
    NotFound { id: String },
    Store(store::StoreError),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => write!(f, "query must not be empty or whitespace-only"),
            Self::NotFound { id } => write!(f, "not found: {id}"),
            Self::Store(e) => write!(f, "store error: {e}"),
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(e) => Some(e),
            _ => None,
        }
    }
}

impl From<store::StoreError> for QueryError {
    fn from(e: store::StoreError) -> Self {
        Self::Store(e)
    }
}

// ── Request types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SymbolQuery {
    pub repo_id: String,
    pub text: String,
    pub filters: QueryFilters,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QueryFilters {
    pub kind: Option<SymbolKind>,
    pub language: Option<String>,
    pub quality_level: Option<QualityLevel>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TextQuery {
    pub repo_id: String,
    pub pattern: String,
    pub filters: QueryFilters,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct FileOutlineRequest {
    pub repo_id: String,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct FileContentRequest {
    pub repo_id: String,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct FileTreeRequest {
    pub repo_id: String,
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RepoOutlineRequest {
    pub repo_id: String,
}

// ── Response types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub meta: QueryMeta,
}

#[derive(Debug, Clone)]
pub struct QueryMeta {
    pub total_candidates: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct ScoredSymbol {
    pub record: SymbolRecord,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct FileOutline {
    pub file: FileRecord,
    pub symbols: Vec<SymbolRecord>,
}

#[derive(Debug, Clone)]
pub struct FileContent {
    pub file: FileRecord,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct FileTreeEntry {
    pub path: String,
    pub language: String,
    pub symbol_count: u64,
}

#[derive(Debug, Clone)]
pub struct RepoOutline {
    pub repo: RepoRecord,
    pub files: Vec<FileTreeEntry>,
}

#[derive(Debug, Clone)]
pub struct TextMatch {
    pub file_path: String,
    pub line_number: u32,
    pub line_content: String,
    pub symbol: Option<SymbolRecord>,
    pub score: f32,
}

// ── Query trait ────────────────────────────────────────────────────────

pub trait QueryService {
    fn search_symbols(&self, query: &SymbolQuery) -> Result<QueryResult<ScoredSymbol>, QueryError>;

    fn get_symbol(&self, id: &str) -> Result<SymbolRecord, QueryError>;

    fn get_symbols(&self, ids: &[&str]) -> Result<Vec<SymbolRecord>, QueryError>;

    fn get_file_outline(&self, request: &FileOutlineRequest) -> Result<FileOutline, QueryError>;

    fn get_file_content(&self, request: &FileContentRequest) -> Result<FileContent, QueryError>;

    fn get_file_tree(&self, request: &FileTreeRequest) -> Result<Vec<FileTreeEntry>, QueryError>;

    fn get_repo_outline(&self, request: &RepoOutlineRequest) -> Result<RepoOutline, QueryError>;

    fn search_text(&self, query: &TextQuery) -> Result<QueryResult<TextMatch>, QueryError>;
}

// ── Validation helpers ─────────────────────────────────────────────────

pub fn validate_query_text(text: &str) -> Result<(), QueryError> {
    if text.trim().is_empty() {
        return Err(QueryError::EmptyQuery);
    }
    Ok(())
}

// ── Test support ───────────────────────────────────────────────────────

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::collections::BTreeMap;

    use core_model::{
        build_symbol_id, FileRecord, QualityLevel, QualityMix, RepoRecord, SymbolKind, SymbolRecord,
    };

    use crate::{
        validate_query_text, FileContent, FileContentRequest, FileOutline, FileOutlineRequest,
        FileTreeEntry, FileTreeRequest, QueryError, QueryMeta, QueryResult, QueryService,
        RepoOutline, RepoOutlineRequest, ScoredSymbol, SymbolQuery, TextMatch, TextQuery,
    };

    pub fn test_symbol(name: &str, kind: SymbolKind) -> SymbolRecord {
        let file_path = "src/lib.rs";
        let qualified_name = format!("crate::{name}");
        SymbolRecord {
            id: build_symbol_id(file_path, &qualified_name, kind).expect("build id"),
            repo_id: "repo-1".into(),
            file_path: file_path.into(),
            language: "rust".into(),
            kind,
            name: name.into(),
            qualified_name,
            signature: format!("fn {name}()"),
            start_line: 1,
            end_line: 5,
            start_byte: 0,
            byte_length: 50,
            content_hash: "hash".into(),
            quality_level: QualityLevel::Syntax,
            confidence_score: 0.8,
            source_adapter: "syntax-treesitter-v1".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            docstring: None,
            summary: None,
            parent_symbol_id: None,
            keywords: None,
            decorators_or_attributes: None,
            semantic_refs: None,
        }
    }

    pub fn test_file(path: &str) -> FileRecord {
        FileRecord {
            repo_id: "repo-1".into(),
            file_path: path.into(),
            language: "rust".into(),
            file_hash: "hash".into(),
            summary: "A source file".into(),
            symbol_count: 3,
            quality_mix: QualityMix {
                semantic_percent: 0.0,
                syntax_percent: 100.0,
            },
            updated_at: "2026-03-09T00:00:00Z".into(),
        }
    }

    pub fn test_repo() -> RepoRecord {
        let mut language_counts = BTreeMap::new();
        language_counts.insert("rust".into(), 5);
        RepoRecord {
            repo_id: "repo-1".into(),
            display_name: "Test".into(),
            source_root: "/tmp/repo".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            index_version: "1.0.0".into(),
            language_counts,
            file_count: 2,
            symbol_count: 5,
            git_head: None,
        }
    }

    /// In-memory stub implementation of [`QueryService`] for testing.
    ///
    /// Pre-populated with three symbols (`alpha`/Function, `beta`/Function,
    /// `Alpha`/Type), two files (`src/lib.rs`, `src/main.rs`), and one repo.
    pub struct StubQueryService {
        pub symbols: Vec<SymbolRecord>,
        pub files: Vec<FileRecord>,
        pub repo: RepoRecord,
    }

    impl StubQueryService {
        pub fn new() -> Self {
            Self {
                symbols: vec![
                    test_symbol("alpha", SymbolKind::Function),
                    test_symbol("beta", SymbolKind::Function),
                    test_symbol("Alpha", SymbolKind::Type),
                ],
                files: vec![test_file("src/lib.rs"), test_file("src/main.rs")],
                repo: test_repo(),
            }
        }
    }

    impl Default for StubQueryService {
        fn default() -> Self {
            Self::new()
        }
    }

    impl QueryService for StubQueryService {
        fn search_symbols(
            &self,
            query: &SymbolQuery,
        ) -> Result<QueryResult<ScoredSymbol>, QueryError> {
            validate_query_text(&query.text)?;

            let text_lower = query.text.to_lowercase();
            let mut scored: Vec<ScoredSymbol> = self
                .symbols
                .iter()
                .filter(|s| s.repo_id == query.repo_id)
                .filter(|s| {
                    if let Some(kind) = query.filters.kind {
                        s.kind == kind
                    } else {
                        true
                    }
                })
                .filter(|s| {
                    if let Some(ref lang) = query.filters.language {
                        &s.language == lang
                    } else {
                        true
                    }
                })
                .filter(|s| {
                    if let Some(ql) = query.filters.quality_level {
                        s.quality_level == ql
                    } else {
                        true
                    }
                })
                .filter(|s| {
                    if let Some(ref fp) = query.filters.file_path {
                        &s.file_path == fp
                    } else {
                        true
                    }
                })
                .filter(|s| s.name.to_lowercase().contains(&text_lower))
                .map(|s| {
                    let exact = s.name.to_lowercase() == text_lower;
                    ScoredSymbol {
                        record: s.clone(),
                        score: if exact { 1.0 } else { 0.5 },
                    }
                })
                .collect();

            let total_candidates = scored.len();

            // Deterministic ordering: score desc, then name asc for tie-breaking.
            scored.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.record.name.cmp(&b.record.name))
            });

            let truncated = total_candidates > query.limit;
            let items: Vec<ScoredSymbol> = scored
                .into_iter()
                .skip(query.offset)
                .take(query.limit)
                .collect();

            Ok(QueryResult {
                items,
                meta: QueryMeta {
                    total_candidates,
                    truncated,
                },
            })
        }

        fn get_symbol(&self, id: &str) -> Result<SymbolRecord, QueryError> {
            self.symbols
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .ok_or_else(|| QueryError::NotFound { id: id.into() })
        }

        fn get_symbols(&self, ids: &[&str]) -> Result<Vec<SymbolRecord>, QueryError> {
            let mut results = Vec::new();
            for id in ids {
                if let Some(s) = self.symbols.iter().find(|s| s.id == *id) {
                    results.push(s.clone());
                }
            }
            Ok(results)
        }

        fn get_file_outline(
            &self,
            request: &FileOutlineRequest,
        ) -> Result<FileOutline, QueryError> {
            let file = self
                .files
                .iter()
                .find(|f| f.repo_id == request.repo_id && f.file_path == request.file_path)
                .cloned()
                .ok_or_else(|| QueryError::NotFound {
                    id: request.file_path.clone(),
                })?;
            let symbols: Vec<SymbolRecord> = self
                .symbols
                .iter()
                .filter(|s| s.repo_id == request.repo_id && s.file_path == request.file_path)
                .cloned()
                .collect();
            Ok(FileOutline { file, symbols })
        }

        fn get_file_content(
            &self,
            request: &FileContentRequest,
        ) -> Result<FileContent, QueryError> {
            let file = self
                .files
                .iter()
                .find(|f| f.repo_id == request.repo_id && f.file_path == request.file_path)
                .cloned()
                .ok_or_else(|| QueryError::NotFound {
                    id: request.file_path.clone(),
                })?;
            Ok(FileContent {
                file,
                content: "// stub content".into(),
            })
        }

        fn get_file_tree(
            &self,
            request: &FileTreeRequest,
        ) -> Result<Vec<FileTreeEntry>, QueryError> {
            let entries: Vec<FileTreeEntry> = self
                .files
                .iter()
                .filter(|f| f.repo_id == request.repo_id)
                .filter(|f| {
                    if let Some(ref prefix) = request.path_prefix {
                        f.file_path.starts_with(prefix)
                    } else {
                        true
                    }
                })
                .map(|f| FileTreeEntry {
                    path: f.file_path.clone(),
                    language: f.language.clone(),
                    symbol_count: f.symbol_count,
                })
                .collect();
            Ok(entries)
        }

        fn get_repo_outline(
            &self,
            request: &RepoOutlineRequest,
        ) -> Result<RepoOutline, QueryError> {
            if self.repo.repo_id != request.repo_id {
                return Err(QueryError::NotFound {
                    id: request.repo_id.clone(),
                });
            }
            let files: Vec<FileTreeEntry> = self
                .files
                .iter()
                .filter(|f| f.repo_id == request.repo_id)
                .map(|f| FileTreeEntry {
                    path: f.file_path.clone(),
                    language: f.language.clone(),
                    symbol_count: f.symbol_count,
                })
                .collect();
            Ok(RepoOutline {
                repo: self.repo.clone(),
                files,
            })
        }

        fn search_text(&self, query: &TextQuery) -> Result<QueryResult<TextMatch>, QueryError> {
            validate_query_text(&query.pattern)?;
            // Stub: no actual text search, return empty results.
            Ok(QueryResult {
                items: vec![],
                meta: QueryMeta {
                    total_candidates: 0,
                    truncated: false,
                },
            })
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_support::StubQueryService;
    use super::*;

    // ── Trait contract tests ───────────────────────────────────────────

    #[test]
    fn empty_query_is_rejected() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "   ".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let err = svc.search_symbols(&query).unwrap_err();
        assert!(matches!(err, QueryError::EmptyQuery));
    }

    #[test]
    fn empty_text_query_is_rejected() {
        let svc = StubQueryService::new();
        let query = TextQuery {
            repo_id: "repo-1".into(),
            pattern: "".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let err = svc.search_text(&query).unwrap_err();
        assert!(matches!(err, QueryError::EmptyQuery));
    }

    #[test]
    fn search_returns_matching_symbols() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "alpha".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert_eq!(result.items.len(), 2); // alpha (fn) + Alpha (type)
    }

    #[test]
    fn search_exact_match_scores_higher() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "beta".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert_eq!(result.items.len(), 1);
        // "beta" matches exactly → score 1.0.
        assert_eq!(result.items[0].record.name, "beta");
        assert!((result.items[0].score - 1.0).abs() < f32::EPSILON);

        // A partial match should score lower.
        let query2 = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "alph".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let result2 = svc.search_symbols(&query2).unwrap();
        assert!(!result2.items.is_empty());
        assert!(result2.items[0].score < 1.0);
    }

    #[test]
    fn search_deterministic_ordering_on_ties() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "a".into(), // matches alpha, Alpha, beta — all partial
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let r1 = svc.search_symbols(&query).unwrap();
        let r2 = svc.search_symbols(&query).unwrap();
        let names1: Vec<&str> = r1.items.iter().map(|s| s.record.name.as_str()).collect();
        let names2: Vec<&str> = r2.items.iter().map(|s| s.record.name.as_str()).collect();
        assert_eq!(names1, names2);
    }

    #[test]
    fn search_truncation_metadata() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "alpha".into(),
            filters: QueryFilters::default(),
            limit: 1,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(result.meta.truncated);
        assert_eq!(result.meta.total_candidates, 2);
    }

    #[test]
    fn search_no_truncation_when_all_fit() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "beta".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(!result.meta.truncated);
    }

    #[test]
    fn search_filters_by_kind() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "alpha".into(),
            filters: QueryFilters {
                kind: Some(SymbolKind::Type),
                ..QueryFilters::default()
            },
            limit: 10,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].record.kind, SymbolKind::Type);
    }

    #[test]
    fn search_filters_by_language() {
        let svc = StubQueryService::new();
        let query = SymbolQuery {
            repo_id: "repo-1".into(),
            text: "alpha".into(),
            filters: QueryFilters {
                language: Some("python".into()),
                ..QueryFilters::default()
            },
            limit: 10,
            offset: 0,
        };
        let result = svc.search_symbols(&query).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn get_symbol_returns_record() {
        let svc = StubQueryService::new();
        let id = &svc.symbols[0].id;
        let record = svc.get_symbol(id).unwrap();
        assert_eq!(&record.id, id);
    }

    #[test]
    fn get_symbol_not_found() {
        let svc = StubQueryService::new();
        let err = svc.get_symbol("nonexistent").unwrap_err();
        assert!(matches!(err, QueryError::NotFound { .. }));
    }

    #[test]
    fn get_symbols_returns_found_records() {
        let svc = StubQueryService::new();
        let ids: Vec<&str> = vec![&svc.symbols[0].id, &svc.symbols[1].id, "missing"];
        let results = svc.get_symbols(&ids).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn get_file_outline_returns_symbols() {
        let svc = StubQueryService::new();
        let outline = svc
            .get_file_outline(&FileOutlineRequest {
                repo_id: "repo-1".into(),
                file_path: "src/lib.rs".into(),
            })
            .unwrap();
        assert_eq!(outline.file.file_path, "src/lib.rs");
        assert_eq!(outline.symbols.len(), 3);
    }

    #[test]
    fn get_file_outline_not_found() {
        let svc = StubQueryService::new();
        let err = svc
            .get_file_outline(&FileOutlineRequest {
                repo_id: "repo-1".into(),
                file_path: "nonexistent.rs".into(),
            })
            .unwrap_err();
        assert!(matches!(err, QueryError::NotFound { .. }));
    }

    #[test]
    fn get_file_content_returns_content() {
        let svc = StubQueryService::new();
        let result = svc
            .get_file_content(&FileContentRequest {
                repo_id: "repo-1".into(),
                file_path: "src/lib.rs".into(),
            })
            .unwrap();
        assert!(!result.content.is_empty());
    }

    #[test]
    fn get_file_tree_returns_all_files() {
        let svc = StubQueryService::new();
        let tree = svc
            .get_file_tree(&FileTreeRequest {
                repo_id: "repo-1".into(),
                path_prefix: None,
            })
            .unwrap();
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn get_file_tree_filters_by_prefix() {
        let svc = StubQueryService::new();
        let tree = svc
            .get_file_tree(&FileTreeRequest {
                repo_id: "repo-1".into(),
                path_prefix: Some("src/main".into()),
            })
            .unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, "src/main.rs");
    }

    #[test]
    fn get_repo_outline_returns_repo_and_files() {
        let svc = StubQueryService::new();
        let outline = svc
            .get_repo_outline(&RepoOutlineRequest {
                repo_id: "repo-1".into(),
            })
            .unwrap();
        assert_eq!(outline.repo.repo_id, "repo-1");
        assert_eq!(outline.files.len(), 2);
    }

    #[test]
    fn get_repo_outline_not_found() {
        let svc = StubQueryService::new();
        let err = svc
            .get_repo_outline(&RepoOutlineRequest {
                repo_id: "unknown".into(),
            })
            .unwrap_err();
        assert!(matches!(err, QueryError::NotFound { .. }));
    }

    #[test]
    fn search_text_empty_pattern_rejected() {
        let svc = StubQueryService::new();
        let err = svc
            .search_text(&TextQuery {
                repo_id: "repo-1".into(),
                pattern: "  ".into(),
                filters: QueryFilters::default(),
                limit: 10,
                offset: 0,
            })
            .unwrap_err();
        assert!(matches!(err, QueryError::EmptyQuery));
    }

    #[test]
    fn query_error_display() {
        assert_eq!(
            QueryError::EmptyQuery.to_string(),
            "query must not be empty or whitespace-only"
        );
        assert!(QueryError::NotFound { id: "x".into() }
            .to_string()
            .contains("not found"));
    }
}

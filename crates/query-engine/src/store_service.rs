//! [`QueryService`] implementation backed by [`MetadataStore`].

use store::MetadataStore;
use tracing::info_span;

use crate::ranking::{score_symbol, sort_scored};
use crate::{
    validate_query_text, FileContent, FileContentRequest, FileOutline, FileOutlineRequest,
    FileTreeEntry, FileTreeRequest, QueryError, QueryMeta, QueryResult, QueryService, RepoOutline,
    RepoOutlineRequest, ScoredSymbol, SymbolQuery, TextMatch, TextQuery,
};
use core_model::SymbolRecord;

/// Production [`QueryService`] implementation backed by a [`MetadataStore`].
pub struct StoreQueryService<'a> {
    store: &'a MetadataStore,
}

impl<'a> StoreQueryService<'a> {
    pub fn new(store: &'a MetadataStore) -> Self {
        Self { store }
    }
}

impl QueryService for StoreQueryService<'_> {
    fn search_symbols(&self, query: &SymbolQuery) -> Result<QueryResult<ScoredSymbol>, QueryError> {
        let span = info_span!(
            "query_search_symbols",
            repo_id = %query.repo_id,
            query_text_redacted = true,
            query_length = query.text.len() as u64,
            limit = query.limit,
        );
        let _guard = span.enter();

        validate_query_text(&query.text)?;

        let candidates = self.store.symbols().search_candidates(
            &query.repo_id,
            query.filters.kind,
            query.filters.language.as_deref(),
            query.filters.quality_level,
            query.filters.file_path.as_deref(),
        )?;

        let mut scored: Vec<ScoredSymbol> = candidates
            .into_iter()
            .filter_map(|record| {
                score_symbol(&query.text, &record).map(|score| ScoredSymbol { record, score })
            })
            .collect();

        let total_candidates = scored.len();
        sort_scored(&mut scored);

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
        let span = info_span!("query_get_symbol", symbol_id = %id);
        let _guard = span.enter();

        self.store
            .symbols()
            .get(id)?
            .ok_or_else(|| QueryError::NotFound { id: id.into() })
    }

    fn get_symbols(&self, ids: &[&str]) -> Result<Vec<SymbolRecord>, QueryError> {
        let span = info_span!("query_get_symbols", count = ids.len());
        let _guard = span.enter();

        let mut results = Vec::new();
        for id in ids {
            if let Some(record) = self.store.symbols().get(id)? {
                results.push(record);
            }
        }
        Ok(results)
    }

    fn get_file_outline(&self, request: &FileOutlineRequest) -> Result<FileOutline, QueryError> {
        let span = info_span!(
            "query_get_file_outline",
            repo_id = %request.repo_id,
            file_path = %request.file_path,
        );
        let _guard = span.enter();

        let file = self
            .store
            .files()
            .get(&request.repo_id, &request.file_path)?
            .ok_or_else(|| QueryError::NotFound {
                id: request.file_path.clone(),
            })?;

        let symbol_ids = self
            .store
            .symbols()
            .list_ids_for_file(&request.repo_id, &request.file_path)?;

        let mut symbols = Vec::with_capacity(symbol_ids.len());
        for id in &symbol_ids {
            if let Some(record) = self.store.symbols().get(id)? {
                symbols.push(record);
            }
        }

        Ok(FileOutline { file, symbols })
    }

    fn get_file_content(&self, request: &FileContentRequest) -> Result<FileContent, QueryError> {
        let span = info_span!(
            "query_get_file_content",
            repo_id = %request.repo_id,
            file_path = %request.file_path,
        );
        let _guard = span.enter();

        let file = self
            .store
            .files()
            .get(&request.repo_id, &request.file_path)?
            .ok_or_else(|| QueryError::NotFound {
                id: request.file_path.clone(),
            })?;

        // File content retrieval is delegated to BlobStore in production.
        // For now, return a placeholder indicating content is not yet wired.
        Ok(FileContent {
            file,
            content: String::new(),
        })
    }

    fn get_file_tree(&self, request: &FileTreeRequest) -> Result<Vec<FileTreeEntry>, QueryError> {
        let span = info_span!(
            "query_get_file_tree",
            repo_id = %request.repo_id,
        );
        let _guard = span.enter();

        let paths = self.store.files().list_paths(&request.repo_id)?;

        let mut entries = Vec::new();
        for path in paths {
            if let Some(ref prefix) = request.path_prefix {
                if !path.starts_with(prefix) {
                    continue;
                }
            }
            if let Some(file) = self.store.files().get(&request.repo_id, &path)? {
                entries.push(FileTreeEntry {
                    path: file.file_path,
                    language: file.language,
                    symbol_count: file.symbol_count,
                });
            }
        }

        Ok(entries)
    }

    fn get_repo_outline(&self, request: &RepoOutlineRequest) -> Result<RepoOutline, QueryError> {
        let span = info_span!(
            "query_get_repo_outline",
            repo_id = %request.repo_id,
        );
        let _guard = span.enter();

        let repo =
            self.store
                .repos()
                .get(&request.repo_id)?
                .ok_or_else(|| QueryError::NotFound {
                    id: request.repo_id.clone(),
                })?;

        let file_entries = self.get_file_tree(&FileTreeRequest {
            repo_id: request.repo_id.clone(),
            path_prefix: None,
        })?;

        Ok(RepoOutline {
            repo,
            files: file_entries,
        })
    }

    fn list_repos(&self) -> Result<Vec<core_model::RepoRecord>, QueryError> {
        let span = info_span!("query_list_repos");
        let _guard = span.enter();

        Ok(self.store.repos().list_all()?)
    }

    fn get_repo_status(&self, repo_id: &str) -> Result<core_model::RepoRecord, QueryError> {
        let span = info_span!("query_get_repo_status", repo_id = %repo_id);
        let _guard = span.enter();

        self.store
            .repos()
            .get(repo_id)?
            .ok_or_else(|| QueryError::NotFound { id: repo_id.into() })
    }

    fn search_text(&self, query: &TextQuery) -> Result<QueryResult<TextMatch>, QueryError> {
        let span = info_span!(
            "query_search_text",
            repo_id = %query.repo_id,
            pattern_redacted = true,
            pattern_length = query.pattern.len() as u64,
            limit = query.limit,
        );
        let _guard = span.enter();

        validate_query_text(&query.pattern)?;

        let (hits, total_candidates) = self.store.symbols().search_text_fts(
            &query.repo_id,
            &query.pattern,
            query.limit,
            query.offset,
        )?;

        let mut items = Vec::with_capacity(hits.len());
        for (id, rank) in &hits {
            if let Some(record) = self.store.symbols().get(id)? {
                // Normalize FTS5 rank (negative, lower = better) to a 0..1 score.
                let score = 1.0 / (1.0 + rank.abs() as f32);
                items.push(TextMatch {
                    file_path: record.file_path.clone(),
                    line_number: record.start_line,
                    line_content: record.signature.clone(),
                    symbol: Some(record),
                    score,
                });
            }
        }

        let truncated = total_candidates > query.limit + query.offset;

        Ok(QueryResult {
            items,
            meta: QueryMeta {
                total_candidates,
                truncated,
            },
        })
    }
}

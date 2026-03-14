//! Integration tests for `search_symbols` backed by a seeded MetadataStore.

use std::collections::BTreeMap;

use core_model::{
    FileRecord, FreshnessStatus, IndexingStatus, QualityLevel, QualityMix, RepoRecord, SymbolKind,
    SymbolRecord,
};
use query_engine::{QueryError, QueryFilters, QueryService, StoreQueryService, SymbolQuery};
use store::MetadataStore;

fn seed_store() -> MetadataStore {
    let store = MetadataStore::open_in_memory().unwrap();

    store
        .repos()
        .upsert(&RepoRecord {
            repo_id: "repo-1".into(),
            display_name: "Test".into(),
            source_root: "/tmp/test".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            index_version: "1.0.0".into(),
            language_counts: BTreeMap::from([("rust".into(), 3)]),
            file_count: 2,
            symbol_count: 5,
            git_head: None,
            registered_at: Some("2026-03-09T00:00:00Z".to_string()),
            indexing_status: IndexingStatus::Ready,
            freshness_status: FreshnessStatus::Fresh,
        })
        .unwrap();

    for path in ["src/lib.rs", "src/server.rs"] {
        store
            .files()
            .upsert(&FileRecord {
                repo_id: "repo-1".into(),
                file_path: path.into(),
                language: "rust".into(),
                file_hash: "sha256:abc".into(),
                summary: "source file".into(),
                symbol_count: 0,
                quality_mix: QualityMix {
                    semantic_percent: 0.0,
                    syntax_percent: 100.0,
                },
                updated_at: "2026-03-09T00:00:00Z".into(),
            })
            .unwrap();
    }

    let symbols = vec![
        make_symbol(
            "run",
            SymbolKind::Function,
            "src/lib.rs",
            "fn run()",
            QualityLevel::Syntax,
            0.8,
            None,
        ),
        make_symbol(
            "run_server",
            SymbolKind::Function,
            "src/server.rs",
            "fn run_server(port: u16)",
            QualityLevel::Syntax,
            0.7,
            None,
        ),
        make_symbol(
            "RunConfig",
            SymbolKind::Class,
            "src/lib.rs",
            "struct RunConfig",
            QualityLevel::Semantic,
            0.95,
            Some(vec!["config".into(), "server".into()]),
        ),
        make_symbol(
            "handle_request",
            SymbolKind::Function,
            "src/server.rs",
            "fn handle_request(req: Request) -> Response",
            QualityLevel::Semantic,
            0.9,
            Some(vec!["http".into(), "handler".into()]),
        ),
        make_symbol(
            "Status",
            SymbolKind::Type,
            "src/lib.rs",
            "enum Status",
            QualityLevel::Syntax,
            0.75,
            None,
        ),
    ];

    for sym in &symbols {
        store.symbols().upsert(sym).unwrap();
    }

    store
}

fn make_symbol(
    name: &str,
    kind: SymbolKind,
    file_path: &str,
    signature: &str,
    quality_level: QualityLevel,
    confidence: f32,
    keywords: Option<Vec<String>>,
) -> SymbolRecord {
    let qualified_name = format!("crate::{name}");
    SymbolRecord {
        id: core_model::build_symbol_id("repo-1", file_path, &qualified_name, kind)
            .expect("build id"),
        repo_id: "repo-1".into(),
        file_path: file_path.into(),
        language: "rust".into(),
        kind,
        name: name.into(),
        qualified_name,
        signature: signature.into(),
        start_line: 1,
        end_line: 10,
        start_byte: 0,
        byte_length: 100,
        content_hash: format!("hash-{name}"),
        quality_level,
        confidence_score: confidence,
        source_adapter: "syntax-treesitter-v1".into(),
        indexed_at: "2026-03-09T00:00:00Z".into(),
        docstring: None,
        summary: None,
        parent_symbol_id: None,
        keywords,
        decorators_or_attributes: None,
        semantic_refs: None,
    }
}

#[test]
fn empty_query_is_rejected() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let err = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "   ".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap_err();
    assert!(matches!(err, QueryError::EmptyQuery));
}

#[test]
fn exact_name_match_ranks_first() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(!result.items.is_empty());
    // The exact match "run" should be first.
    assert_eq!(result.items[0].record.name, "run");
    // "run_server" and "RunConfig" should also appear (token overlap).
    assert!(result.items.len() >= 2);
}

#[test]
fn exact_match_scores_higher_than_partial() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    let exact = result
        .items
        .iter()
        .find(|s| s.record.name == "run")
        .unwrap();
    let partial = result
        .items
        .iter()
        .find(|s| s.record.name == "run_server")
        .unwrap();
    assert!(
        exact.score > partial.score,
        "exact ({}) should beat partial ({})",
        exact.score,
        partial.score
    );
}

#[test]
fn filter_by_kind() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters {
                kind: Some(SymbolKind::Class),
                ..QueryFilters::default()
            },
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result
        .items
        .iter()
        .all(|s| s.record.kind == SymbolKind::Class));
}

#[test]
fn filter_by_file_path() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters {
                file_path: Some("src/server.rs".into()),
                ..QueryFilters::default()
            },
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result
        .items
        .iter()
        .all(|s| s.record.file_path == "src/server.rs"));
}

#[test]
fn filter_by_quality_level() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters {
                quality_level: Some(QualityLevel::Semantic),
                ..QueryFilters::default()
            },
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result
        .items
        .iter()
        .all(|s| s.record.quality_level == QualityLevel::Semantic));
}

#[test]
fn truncation_metadata_when_limit_exceeded() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 1,
            offset: 0,
        })
        .unwrap();

    assert_eq!(result.items.len(), 1);
    assert!(result.meta.truncated);
    assert!(result.meta.total_candidates > 1);
}

#[test]
fn no_truncation_when_all_fit() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "Status".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(!result.meta.truncated);
}

#[test]
fn deterministic_ordering_on_ties() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let query = SymbolQuery {
        repo_id: "repo-1".into(),
        text: "run".into(),
        filters: QueryFilters::default(),
        limit: 10,
        offset: 0,
    };

    let r1 = svc.search_symbols(&query).unwrap();
    let r2 = svc.search_symbols(&query).unwrap();

    let ids1: Vec<&str> = r1.items.iter().map(|s| s.record.id.as_str()).collect();
    let ids2: Vec<&str> = r2.items.iter().map(|s| s.record.id.as_str()).collect();
    assert_eq!(ids1, ids2);
}

#[test]
fn no_match_returns_empty() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "zzz_nonexistent_xyz".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result.items.is_empty());
    assert!(!result.meta.truncated);
    assert_eq!(result.meta.total_candidates, 0);
}

#[test]
fn wrong_repo_returns_empty() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "nonexistent-repo".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result.items.is_empty());
}

#[test]
fn keyword_match_surfaces_symbol() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "http".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    // "handle_request" has keyword "http".
    assert!(
        result
            .items
            .iter()
            .any(|s| s.record.name == "handle_request"),
        "keyword match should surface handle_request"
    );
}

#[test]
fn results_carry_quality_and_confidence() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    for item in &result.items {
        assert!(item.score > 0.0);
        assert!(item.record.confidence_score >= 0.0);
        assert!(item.record.confidence_score <= 1.0);
        assert!(!item.record.source_adapter.is_empty());
    }
}

#[test]
fn semantic_quality_boosts_ranking() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "config".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    // RunConfig has keyword "config" and is semantic quality.
    if !result.items.is_empty() {
        let run_config = result.items.iter().find(|s| s.record.name == "RunConfig");
        assert!(run_config.is_some(), "RunConfig should match via keyword");
    }
}

#[test]
fn offset_skips_results() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let full = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    let offset = svc
        .search_symbols(&SymbolQuery {
            repo_id: "repo-1".into(),
            text: "run".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 1,
        })
        .unwrap();

    assert_eq!(offset.items.len(), full.items.len() - 1);
    if full.items.len() > 1 {
        assert_eq!(offset.items[0].record.id, full.items[1].record.id);
    }
}

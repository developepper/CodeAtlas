//! Integration tests for `get_symbol` and `get_symbols` backed by a seeded MetadataStore.

use std::collections::BTreeMap;

use core_model::{FileRecord, QualityLevel, QualityMix, RepoRecord, SymbolKind, SymbolRecord};
use query_engine::{QueryError, QueryService, StoreQueryService};
use store::MetadataStore;

fn seed_store() -> (MetadataStore, Vec<SymbolRecord>) {
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
            file_count: 1,
            symbol_count: 3,
            git_head: None,
        })
        .unwrap();

    store
        .files()
        .upsert(&FileRecord {
            repo_id: "repo-1".into(),
            file_path: "src/lib.rs".into(),
            language: "rust".into(),
            file_hash: "sha256:abc".into(),
            summary: "source file".into(),
            symbol_count: 3,
            quality_mix: QualityMix {
                semantic_percent: 50.0,
                syntax_percent: 50.0,
            },
            updated_at: "2026-03-09T00:00:00Z".into(),
        })
        .unwrap();

    let symbols = vec![
        make_symbol("alpha", SymbolKind::Function, QualityLevel::Syntax, 0.8),
        make_symbol("Beta", SymbolKind::Class, QualityLevel::Semantic, 0.95),
        make_symbol("GAMMA", SymbolKind::Constant, QualityLevel::Syntax, 0.7),
    ];

    for sym in &symbols {
        store.symbols().upsert(sym).unwrap();
    }

    (store, symbols)
}

fn make_symbol(
    name: &str,
    kind: SymbolKind,
    quality_level: QualityLevel,
    confidence: f32,
) -> SymbolRecord {
    let file_path = "src/lib.rs";
    let qualified_name = format!("crate::{name}");
    SymbolRecord {
        id: core_model::build_symbol_id(file_path, &qualified_name, kind).expect("build id"),
        repo_id: "repo-1".into(),
        file_path: file_path.into(),
        language: "rust".into(),
        kind,
        name: name.into(),
        qualified_name,
        signature: format!("fn {name}()"),
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
        keywords: None,
        decorators_or_attributes: None,
        semantic_refs: None,
    }
}

// ── get_symbol ─────────────────────────────────────────────────────────

#[test]
fn get_symbol_returns_matching_record() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.get_symbol(&symbols[0].id).unwrap();
    assert_eq!(result.id, symbols[0].id);
    assert_eq!(result.name, "alpha");
    assert_eq!(result.kind, SymbolKind::Function);
}

#[test]
fn get_symbol_returns_all_fields() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.get_symbol(&symbols[1].id).unwrap();
    assert_eq!(result.id, symbols[1].id);
    assert_eq!(result.repo_id, "repo-1");
    assert_eq!(result.file_path, "src/lib.rs");
    assert_eq!(result.language, "rust");
    assert_eq!(result.kind, SymbolKind::Class);
    assert_eq!(result.name, "Beta");
    assert_eq!(result.qualified_name, "crate::Beta");
    assert_eq!(result.quality_level, QualityLevel::Semantic);
    assert!((result.confidence_score - 0.95).abs() < f32::EPSILON);
    assert_eq!(result.source_adapter, "syntax-treesitter-v1");
}

#[test]
fn get_symbol_not_found_returns_error() {
    let (store, _) = seed_store();
    let svc = StoreQueryService::new(&store);

    let err = svc.get_symbol("nonexistent::id#function").unwrap_err();
    match err {
        QueryError::NotFound { id } => assert_eq!(id, "nonexistent::id#function"),
        other => panic!("expected NotFound, got: {other}"),
    }
}

#[test]
fn get_symbol_empty_id_returns_not_found() {
    let (store, _) = seed_store();
    let svc = StoreQueryService::new(&store);

    let err = svc.get_symbol("").unwrap_err();
    assert!(matches!(err, QueryError::NotFound { .. }));
}

// ── get_symbols ────────────────────────────────────────────────────────

#[test]
fn get_symbols_returns_all_found() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let ids: Vec<&str> = symbols.iter().map(|s| s.id.as_str()).collect();
    let results = svc.get_symbols(&ids).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn get_symbols_skips_missing_ids() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let ids = vec![symbols[0].id.as_str(), "missing::id#function"];
    let results = svc.get_symbols(&ids).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, symbols[0].id);
}

#[test]
fn get_symbols_empty_input_returns_empty() {
    let (store, _) = seed_store();
    let svc = StoreQueryService::new(&store);

    let results = svc.get_symbols(&[]).unwrap();
    assert!(results.is_empty());
}

#[test]
fn get_symbols_all_missing_returns_empty() {
    let (store, _) = seed_store();
    let svc = StoreQueryService::new(&store);

    let results = svc
        .get_symbols(&["missing::a#function", "missing::b#type"])
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn get_symbols_preserves_request_order() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    // Request in reverse order.
    let ids = vec![symbols[2].id.as_str(), symbols[0].id.as_str()];
    let results = svc.get_symbols(&ids).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, symbols[2].id);
    assert_eq!(results[1].id, symbols[0].id);
}

#[test]
fn get_symbols_duplicate_ids_returns_duplicates() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let ids = vec![symbols[0].id.as_str(), symbols[0].id.as_str()];
    let results = svc.get_symbols(&ids).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, results[1].id);
}

#[test]
fn get_symbol_deterministic_across_calls() {
    let (store, symbols) = seed_store();
    let svc = StoreQueryService::new(&store);

    let r1 = svc.get_symbol(&symbols[0].id).unwrap();
    let r2 = svc.get_symbol(&symbols[0].id).unwrap();
    assert_eq!(r1, r2);
}

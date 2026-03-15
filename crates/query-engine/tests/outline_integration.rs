//! Integration tests for file/repo outline and tree queries.

use std::collections::BTreeMap;

use core_model::{
    FileRecord, FreshnessStatus, IndexingStatus, QualityLevel, QualityMix, RepoRecord, SymbolKind,
    SymbolRecord,
};
use query_engine::{
    FileContentRequest, FileOutlineRequest, FileTreeRequest, QueryError, QueryService,
    RepoOutlineRequest, StoreQueryService,
};
use store::MetadataStore;
use tempfile::TempDir;

fn seed_store() -> (MetadataStore, store::BlobStore, TempDir) {
    let store = MetadataStore::open_in_memory().unwrap();

    store
        .repos()
        .upsert(&RepoRecord {
            repo_id: "repo-1".into(),
            display_name: "TestProject".into(),
            source_root: "/tmp/test".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            index_version: "1.0.0".into(),
            language_counts: BTreeMap::from([("rust".into(), 3), ("toml".into(), 1)]),
            file_count: 4,
            symbol_count: 5,
            git_head: Some("abc123".into()),
            registered_at: Some("2026-03-09T00:00:00Z".to_string()),
            indexing_status: IndexingStatus::Ready,
            freshness_status: FreshnessStatus::Fresh,
        })
        .unwrap();

    let blob_dir = TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();

    let files = vec![
        ("src/lib.rs", "rust", 3, "pub fn run() {}\n"),
        ("src/server.rs", "rust", 2, "pub fn handle() {}\n"),
        ("src/util/helpers.rs", "rust", 0, "// helpers\n"),
        ("Cargo.toml", "toml", 0, "[package]\nname = \"test\"\n"),
    ];

    for (path, lang, sym_count, content) in &files {
        let hash = store::content_hash(content.as_bytes());
        blob_store.put(content.as_bytes()).unwrap();
        store
            .files()
            .upsert(&FileRecord {
                repo_id: "repo-1".into(),
                file_path: (*path).into(),
                language: (*lang).into(),
                file_hash: hash,
                summary: format!("{path} source file"),
                symbol_count: *sym_count,
                quality_mix: QualityMix {
                    semantic_percent: 0.0,
                    syntax_percent: 100.0,
                },
                updated_at: "2026-03-09T00:00:00Z".into(),
            })
            .unwrap();
    }

    // Symbols in src/lib.rs
    for (name, kind) in [
        ("Config", SymbolKind::Class),
        ("run", SymbolKind::Function),
        ("Status", SymbolKind::Type),
    ] {
        store
            .symbols()
            .upsert(&make_symbol(name, kind, "src/lib.rs"))
            .unwrap();
    }

    // Symbols in src/server.rs
    for (name, kind) in [
        ("handle", SymbolKind::Function),
        ("Server", SymbolKind::Class),
    ] {
        store
            .symbols()
            .upsert(&make_symbol(name, kind, "src/server.rs"))
            .unwrap();
    }

    (store, blob_store, blob_dir)
}

fn make_symbol(name: &str, kind: SymbolKind, file_path: &str) -> SymbolRecord {
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
        signature: format!("fn {name}()"),
        start_line: 1,
        end_line: 10,
        start_byte: 0,
        byte_length: 100,
        content_hash: format!("hash-{name}"),
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

// ── get_file_outline ───────────────────────────────────────────────────

#[test]
fn file_outline_returns_file_and_symbols() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "repo-1".into(),
            file_path: "src/lib.rs".into(),
        })
        .unwrap();

    assert_eq!(outline.file.file_path, "src/lib.rs");
    assert_eq!(outline.file.language, "rust");
    assert_eq!(outline.symbols.len(), 3);
}

#[test]
fn file_outline_symbols_belong_to_requested_file() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "repo-1".into(),
            file_path: "src/server.rs".into(),
        })
        .unwrap();

    assert_eq!(outline.symbols.len(), 2);
    for sym in &outline.symbols {
        assert_eq!(sym.file_path, "src/server.rs");
    }
}

#[test]
fn file_outline_empty_file_returns_no_symbols() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "repo-1".into(),
            file_path: "src/util/helpers.rs".into(),
        })
        .unwrap();

    assert_eq!(outline.file.file_path, "src/util/helpers.rs");
    assert!(outline.symbols.is_empty());
}

#[test]
fn file_outline_not_found() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let err = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "repo-1".into(),
            file_path: "nonexistent.rs".into(),
        })
        .unwrap_err();

    match err {
        QueryError::NotFound { id } => assert_eq!(id, "nonexistent.rs"),
        other => panic!("expected NotFound, got: {other}"),
    }
}

#[test]
fn file_outline_wrong_repo_not_found() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let err = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "wrong-repo".into(),
            file_path: "src/lib.rs".into(),
        })
        .unwrap_err();

    assert!(matches!(err, QueryError::NotFound { .. }));
}

#[test]
fn file_outline_deterministic_across_calls() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);
    let req = FileOutlineRequest {
        repo_id: "repo-1".into(),
        file_path: "src/lib.rs".into(),
    };

    let o1 = svc.get_file_outline(&req).unwrap();
    let o2 = svc.get_file_outline(&req).unwrap();

    let ids1: Vec<&str> = o1.symbols.iter().map(|s| s.id.as_str()).collect();
    let ids2: Vec<&str> = o2.symbols.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids1, ids2);
}

// ── get_file_content ───────────────────────────────────────────────────

#[test]
fn file_content_returns_file_record_and_content() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let result = svc
        .get_file_content(&FileContentRequest {
            repo_id: "repo-1".into(),
            file_path: "src/lib.rs".into(),
        })
        .unwrap();

    assert_eq!(result.file.file_path, "src/lib.rs");
    assert_eq!(result.file.language, "rust");
    assert!(
        result.content.contains("pub fn run()"),
        "content should contain actual source code"
    );
}

#[test]
fn file_content_not_found() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let err = svc
        .get_file_content(&FileContentRequest {
            repo_id: "repo-1".into(),
            file_path: "missing.rs".into(),
        })
        .unwrap_err();

    assert!(matches!(err, QueryError::NotFound { .. }));
}

// ── get_file_tree ──────────────────────────────────────────────────────

#[test]
fn file_tree_returns_all_files() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "repo-1".into(),
            path_prefix: None,
        })
        .unwrap();

    assert_eq!(tree.len(), 4);
}

#[test]
fn file_tree_filters_by_prefix() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "repo-1".into(),
            path_prefix: Some("src/".into()),
        })
        .unwrap();

    assert_eq!(tree.len(), 3);
    for entry in &tree {
        assert!(entry.path.starts_with("src/"), "path: {}", entry.path);
    }
}

#[test]
fn file_tree_nested_prefix() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "repo-1".into(),
            path_prefix: Some("src/util/".into()),
        })
        .unwrap();

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].path, "src/util/helpers.rs");
}

#[test]
fn file_tree_no_match_prefix_returns_empty() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "repo-1".into(),
            path_prefix: Some("nonexistent/".into()),
        })
        .unwrap();

    assert!(tree.is_empty());
}

#[test]
fn file_tree_wrong_repo_returns_empty() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "wrong-repo".into(),
            path_prefix: None,
        })
        .unwrap();

    assert!(tree.is_empty());
}

#[test]
fn file_tree_entries_carry_language_and_symbol_count() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let tree = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "repo-1".into(),
            path_prefix: None,
        })
        .unwrap();

    let lib = tree.iter().find(|e| e.path == "src/lib.rs").unwrap();
    assert_eq!(lib.language, "rust");
    assert_eq!(lib.symbol_count, 3);

    let toml = tree.iter().find(|e| e.path == "Cargo.toml").unwrap();
    assert_eq!(toml.language, "toml");
    assert_eq!(toml.symbol_count, 0);
}

#[test]
fn file_tree_order_is_deterministic() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);
    let req = FileTreeRequest {
        repo_id: "repo-1".into(),
        path_prefix: None,
    };

    let t1 = svc.get_file_tree(&req).unwrap();
    let t2 = svc.get_file_tree(&req).unwrap();

    let paths1: Vec<&str> = t1.iter().map(|e| e.path.as_str()).collect();
    let paths2: Vec<&str> = t2.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths1, paths2);
}

// ── get_repo_outline ───────────────────────────────────────────────────

#[test]
fn repo_outline_returns_repo_and_all_files() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_repo_outline(&RepoOutlineRequest {
            repo_id: "repo-1".into(),
        })
        .unwrap();

    assert_eq!(outline.repo.repo_id, "repo-1");
    assert_eq!(outline.repo.display_name, "TestProject");
    assert_eq!(outline.repo.git_head, Some("abc123".into()));
    assert_eq!(outline.files.len(), 4);
}

#[test]
fn repo_outline_carries_language_counts() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_repo_outline(&RepoOutlineRequest {
            repo_id: "repo-1".into(),
        })
        .unwrap();

    assert_eq!(outline.repo.language_counts.get("rust"), Some(&3));
    assert_eq!(outline.repo.language_counts.get("toml"), Some(&1));
}

#[test]
fn repo_outline_not_found() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let err = svc
        .get_repo_outline(&RepoOutlineRequest {
            repo_id: "nonexistent".into(),
        })
        .unwrap_err();

    match err {
        QueryError::NotFound { id } => assert_eq!(id, "nonexistent"),
        other => panic!("expected NotFound, got: {other}"),
    }
}

#[test]
fn repo_outline_deterministic_across_calls() {
    let (store, blob_store, _blob_dir) = seed_store();
    let svc = StoreQueryService::new(&store, &blob_store);
    let req = RepoOutlineRequest {
        repo_id: "repo-1".into(),
    };

    let o1 = svc.get_repo_outline(&req).unwrap();
    let o2 = svc.get_repo_outline(&req).unwrap();

    let paths1: Vec<&str> = o1.files.iter().map(|f| f.path.as_str()).collect();
    let paths2: Vec<&str> = o2.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths1, paths2);
}

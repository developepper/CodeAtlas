//! Integration tests for query behavior across capability tiers.
//!
//! Validates that file outline, symbol search, file tree, and repo outline
//! behave correctly when a repo contains files at different capability tiers:
//! file-only, syntax-only, and syntax-plus-semantic.

use std::collections::BTreeMap;

use core_model::{
    CapabilityTier, FileRecord, FreshnessStatus, IndexingStatus, RepoRecord, SymbolKind,
    SymbolRecord,
};
use query_engine::{
    FileContentRequest, FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService,
    RepoOutlineRequest, StoreQueryService, SymbolQuery,
};
use store::MetadataStore;
use tempfile::TempDir;

/// Seeds a store with a mixed-tier repository:
///
/// - `README.md`: file-only (no symbols)
/// - `src/main.rs`: syntax-only (Rust symbols)
/// - `src/app.ts`: syntax-plus-semantic (TypeScript symbols with semantic enrichment)
fn seed_multi_tier_store() -> (MetadataStore, store::BlobStore, TempDir) {
    let store = MetadataStore::open_in_memory().unwrap();

    store
        .repos()
        .upsert(&RepoRecord {
            repo_id: "multi-tier".into(),
            display_name: "MultiTier".into(),
            source_root: "/tmp/multi".into(),
            indexed_at: "2026-03-16T00:00:00Z".into(),
            index_version: "1.1.0".into(),
            language_counts: BTreeMap::from([
                ("rust".into(), 1),
                ("typescript".into(), 1),
                ("markdown".into(), 1),
            ]),
            file_count: 3,
            symbol_count: 4,
            git_head: None,
            registered_at: Some("2026-03-16T00:00:00Z".to_string()),
            indexing_status: IndexingStatus::Ready,
            freshness_status: FreshnessStatus::Fresh,
        })
        .unwrap();

    let blob_dir = TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();

    // File-only file (no syntax backend for markdown).
    let readme_content = b"# Hello\n";
    let readme_hash = store::content_hash(readme_content);
    blob_store.put(readme_content).unwrap();
    store
        .files()
        .upsert(&FileRecord {
            repo_id: "multi-tier".into(),
            file_path: "README.md".into(),
            language: "markdown".into(),
            file_hash: readme_hash,
            summary: "Readme file".into(),
            symbol_count: 0,
            capability_tier: CapabilityTier::FileOnly,
            updated_at: "2026-03-16T00:00:00Z".into(),
        })
        .unwrap();

    // Syntax-only file (Rust).
    let rs_content = b"pub fn greet() {}\npub struct Config {}\n";
    let rs_hash = store::content_hash(rs_content);
    blob_store.put(rs_content).unwrap();
    store
        .files()
        .upsert(&FileRecord {
            repo_id: "multi-tier".into(),
            file_path: "src/main.rs".into(),
            language: "rust".into(),
            file_hash: rs_hash,
            summary: "Main source".into(),
            symbol_count: 2,
            capability_tier: CapabilityTier::SyntaxOnly,
            updated_at: "2026-03-16T00:00:00Z".into(),
        })
        .unwrap();

    // Syntax-plus-semantic file (TypeScript).
    let ts_content = b"export function hello() {}\nexport class App {}\n";
    let ts_hash = store::content_hash(ts_content);
    blob_store.put(ts_content).unwrap();
    store
        .files()
        .upsert(&FileRecord {
            repo_id: "multi-tier".into(),
            file_path: "src/app.ts".into(),
            language: "typescript".into(),
            file_hash: ts_hash,
            summary: "App source".into(),
            symbol_count: 2,
            capability_tier: CapabilityTier::SyntaxPlusSemantic,
            updated_at: "2026-03-16T00:00:00Z".into(),
        })
        .unwrap();

    // Syntax-only symbols for Rust.
    for (name, kind) in [
        ("greet", SymbolKind::Function),
        ("Config", SymbolKind::Class),
    ] {
        store
            .symbols()
            .upsert(&make_symbol(
                name,
                kind,
                "src/main.rs",
                "rust",
                CapabilityTier::SyntaxOnly,
                0.8,
                "syntax-rust",
            ))
            .unwrap();
    }

    // Syntax-plus-semantic symbols for TypeScript.
    for (name, kind) in [("hello", SymbolKind::Function), ("App", SymbolKind::Class)] {
        store
            .symbols()
            .upsert(&make_symbol(
                name,
                kind,
                "src/app.ts",
                "typescript",
                CapabilityTier::SyntaxPlusSemantic,
                0.95,
                "semantic-typescript",
            ))
            .unwrap();
    }

    (store, blob_store, blob_dir)
}

fn make_symbol(
    name: &str,
    kind: SymbolKind,
    file_path: &str,
    language: &str,
    capability_tier: CapabilityTier,
    confidence: f32,
    source_backend: &str,
) -> SymbolRecord {
    let qualified_name = format!("crate::{name}");
    SymbolRecord {
        id: core_model::build_symbol_id("multi-tier", file_path, &qualified_name, kind)
            .expect("build id"),
        repo_id: "multi-tier".into(),
        file_path: file_path.into(),
        language: language.into(),
        kind,
        name: name.into(),
        qualified_name,
        signature: format!("fn {name}()"),
        start_line: 1,
        end_line: 5,
        start_byte: 0,
        byte_length: 50,
        content_hash: format!("hash-{name}"),
        capability_tier,
        confidence_score: confidence,
        source_backend: source_backend.into(),
        indexed_at: "2026-03-16T00:00:00Z".into(),
        docstring: None,
        summary: None,
        parent_symbol_id: None,
        keywords: None,
        decorators_or_attributes: None,
        semantic_refs: None,
        container_symbol_id: None,
        namespace_path: None,
        raw_kind: None,
        modifiers: None,
    }
}

// ── File outline across tiers ─────────────────────────────────────────

#[test]
fn file_outline_file_only_returns_empty_symbols() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "multi-tier".into(),
            file_path: "README.md".into(),
        })
        .unwrap();

    assert_eq!(outline.file.capability_tier, CapabilityTier::FileOnly);
    assert!(
        outline.symbols.is_empty(),
        "file-only file should have no symbols"
    );
}

#[test]
fn file_outline_syntax_only_returns_syntax_symbols() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "multi-tier".into(),
            file_path: "src/main.rs".into(),
        })
        .unwrap();

    assert_eq!(outline.file.capability_tier, CapabilityTier::SyntaxOnly);
    assert_eq!(outline.symbols.len(), 2);
    assert!(outline
        .symbols
        .iter()
        .all(|s| s.capability_tier == CapabilityTier::SyntaxOnly));
}

#[test]
fn file_outline_syntax_plus_semantic_returns_merged_symbols() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_file_outline(&FileOutlineRequest {
            repo_id: "multi-tier".into(),
            file_path: "src/app.ts".into(),
        })
        .unwrap();

    assert_eq!(
        outline.file.capability_tier,
        CapabilityTier::SyntaxPlusSemantic
    );
    assert_eq!(outline.symbols.len(), 2);
    assert!(outline
        .symbols
        .iter()
        .all(|s| s.capability_tier == CapabilityTier::SyntaxPlusSemantic));
}

// ── Symbol search across tiers ────────────────────────────────────────

#[test]
fn search_symbols_returns_results_from_all_tiers() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    // Search for a broad term that matches symbols in both tiers.
    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "multi-tier".into(),
            text: "Config App greet hello".into(),
            filters: QueryFilters::default(),
            limit: 20,
            offset: 0,
        })
        .unwrap();

    // Should find symbols from both syntax-only and syntax-plus-semantic files.
    let tiers: Vec<CapabilityTier> = result
        .items
        .iter()
        .map(|s| s.record.capability_tier)
        .collect();
    assert!(
        tiers.contains(&CapabilityTier::SyntaxOnly),
        "should contain syntax-only symbols"
    );
    assert!(
        tiers.contains(&CapabilityTier::SyntaxPlusSemantic),
        "should contain syntax-plus-semantic symbols"
    );
}

#[test]
fn search_symbols_filter_by_syntax_only() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let result = svc
        .search_symbols(&SymbolQuery {
            repo_id: "multi-tier".into(),
            text: "greet Config hello App".into(),
            filters: QueryFilters {
                capability_tier: Some(CapabilityTier::SyntaxOnly),
                ..QueryFilters::default()
            },
            limit: 20,
            offset: 0,
        })
        .unwrap();

    assert!(!result.items.is_empty());
    assert!(
        result
            .items
            .iter()
            .all(|s| s.record.capability_tier == CapabilityTier::SyntaxOnly),
        "all results should be syntax-only when filtered"
    );
}

// ── Symbol lookup across tiers ────────────────────────────────────────

#[test]
fn get_symbol_works_for_syntax_only() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let greet_id = core_model::build_symbol_id(
        "multi-tier",
        "src/main.rs",
        "crate::greet",
        SymbolKind::Function,
    )
    .unwrap();

    let record = svc.get_symbol(&greet_id).unwrap();
    assert_eq!(record.name, "greet");
    assert_eq!(record.capability_tier, CapabilityTier::SyntaxOnly);
}

#[test]
fn get_symbol_works_for_syntax_plus_semantic() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let app_id =
        core_model::build_symbol_id("multi-tier", "src/app.ts", "crate::App", SymbolKind::Class)
            .unwrap();

    let record = svc.get_symbol(&app_id).unwrap();
    assert_eq!(record.name, "App");
    assert_eq!(record.capability_tier, CapabilityTier::SyntaxPlusSemantic);
}

// ── File content across tiers ─────────────────────────────────────────

#[test]
fn file_content_returns_content_for_file_only() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let content = svc
        .get_file_content(&FileContentRequest {
            repo_id: "multi-tier".into(),
            file_path: "README.md".into(),
        })
        .unwrap();

    assert_eq!(content.file.capability_tier, CapabilityTier::FileOnly);
    assert!(content.content.contains("Hello"));
}

#[test]
fn file_content_returns_content_for_syntax_only() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let content = svc
        .get_file_content(&FileContentRequest {
            repo_id: "multi-tier".into(),
            file_path: "src/main.rs".into(),
        })
        .unwrap();

    assert_eq!(content.file.capability_tier, CapabilityTier::SyntaxOnly);
    assert!(content.content.contains("greet"));
}

// ── File tree across tiers ────────────────────────────────────────────

#[test]
fn file_tree_entries_carry_capability_tier() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let entries = svc
        .get_file_tree(&FileTreeRequest {
            repo_id: "multi-tier".into(),
            path_prefix: None,
        })
        .unwrap();

    assert_eq!(entries.len(), 3);

    let readme = entries.iter().find(|e| e.path == "README.md").unwrap();
    assert_eq!(readme.capability_tier, CapabilityTier::FileOnly);
    assert_eq!(readme.symbol_count, 0);

    let rs = entries.iter().find(|e| e.path == "src/main.rs").unwrap();
    assert_eq!(rs.capability_tier, CapabilityTier::SyntaxOnly);
    assert_eq!(rs.symbol_count, 2);

    let ts = entries.iter().find(|e| e.path == "src/app.ts").unwrap();
    assert_eq!(ts.capability_tier, CapabilityTier::SyntaxPlusSemantic);
    assert_eq!(ts.symbol_count, 2);
}

// ── Repo outline across tiers ─────────────────────────────────────────

#[test]
fn repo_outline_files_carry_capability_tier() {
    let (store, blob_store, _dir) = seed_multi_tier_store();
    let svc = StoreQueryService::new(&store, &blob_store);

    let outline = svc
        .get_repo_outline(&RepoOutlineRequest {
            repo_id: "multi-tier".into(),
        })
        .unwrap();

    assert_eq!(outline.files.len(), 3);

    let tiers: Vec<CapabilityTier> = outline.files.iter().map(|f| f.capability_tier).collect();
    assert!(tiers.contains(&CapabilityTier::FileOnly));
    assert!(tiers.contains(&CapabilityTier::SyntaxOnly));
    assert!(tiers.contains(&CapabilityTier::SyntaxPlusSemantic));
}

//! Integration tests for `search_text` FTS5-backed full-text search.

use std::collections::BTreeMap;

use core_model::{
    FileRecord, FreshnessStatus, IndexingStatus, QualityLevel, QualityMix, RepoRecord, SymbolKind,
    SymbolRecord,
};
use query_engine::{QueryError, QueryFilters, QueryService, StoreQueryService, TextQuery};
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
            language_counts: BTreeMap::from([("rust".into(), 2)]),
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
                file_hash: format!("sha256:{path}"),
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
            "parse_config",
            SymbolKind::Function,
            "src/lib.rs",
            "fn parse_config(path: &str) -> Config",
            Some("Parses a TOML configuration file."),
            Some(vec!["config".into(), "toml".into(), "parser".into()]),
        ),
        make_symbol(
            "validate_input",
            SymbolKind::Function,
            "src/lib.rs",
            "fn validate_input(data: &[u8]) -> Result<(), Error>",
            Some("Validates user input bytes."),
            Some(vec!["validation".into(), "input".into()]),
        ),
        make_symbol(
            "HttpServer",
            SymbolKind::Class,
            "src/server.rs",
            "struct HttpServer",
            Some("An HTTP server implementation."),
            Some(vec!["http".into(), "server".into(), "networking".into()]),
        ),
        make_symbol(
            "handle_request",
            SymbolKind::Function,
            "src/server.rs",
            "fn handle_request(req: Request) -> Response",
            Some("Handles incoming HTTP requests."),
            Some(vec!["http".into(), "handler".into(), "request".into()]),
        ),
        make_symbol(
            "Status",
            SymbolKind::Type,
            "src/lib.rs",
            "enum Status",
            None,
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
    docstring: Option<&str>,
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
        quality_level: QualityLevel::Syntax,
        confidence_score: 0.8,
        source_adapter: "syntax-treesitter-v1".into(),
        indexed_at: "2026-03-09T00:00:00Z".into(),
        docstring: docstring.map(String::from),
        summary: None,
        parent_symbol_id: None,
        keywords,
        decorators_or_attributes: None,
        semantic_refs: None,
    }
}

fn query(pattern: &str) -> TextQuery {
    TextQuery {
        repo_id: "repo-1".into(),
        pattern: pattern.into(),
        filters: QueryFilters::default(),
        limit: 10,
        offset: 0,
    }
}

// ── Basic matching ─────────────────────────────────────────────────────

#[test]
fn search_text_matches_by_name() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("parse_config")).unwrap();
    assert!(!result.items.is_empty());
    assert!(result
        .items
        .iter()
        .any(|m| m.symbol.as_ref().unwrap().name == "parse_config"));
}

#[test]
fn search_text_matches_by_docstring() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("TOML configuration")).unwrap();
    assert!(!result.items.is_empty());
    assert!(result
        .items
        .iter()
        .any(|m| m.symbol.as_ref().unwrap().name == "parse_config"));
}

#[test]
fn search_text_matches_by_keyword() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("networking")).unwrap();
    assert!(!result.items.is_empty());
    assert!(result
        .items
        .iter()
        .any(|m| m.symbol.as_ref().unwrap().name == "HttpServer"));
}

#[test]
fn search_text_matches_by_signature() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("Request Response")).unwrap();
    assert!(!result.items.is_empty());
}

#[test]
fn search_text_matches_by_qualified_name() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("crate validate_input")).unwrap();
    assert!(!result.items.is_empty());
    assert!(result
        .items
        .iter()
        .any(|m| m.symbol.as_ref().unwrap().name == "validate_input"));
}

// ── Edge cases ─────────────────────────────────────────────────────────

#[test]
fn search_text_empty_query_rejected() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let err = svc
        .search_text(&TextQuery {
            repo_id: "repo-1".into(),
            pattern: "   ".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap_err();

    assert!(matches!(err, QueryError::EmptyQuery));
}

#[test]
fn search_text_no_match_returns_empty() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc
        .search_text(&query("xyzzy_completely_unrelated"))
        .unwrap();
    assert!(result.items.is_empty());
    assert_eq!(result.meta.total_candidates, 0);
    assert!(!result.meta.truncated);
}

#[test]
fn search_text_wrong_repo_returns_empty() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc
        .search_text(&TextQuery {
            repo_id: "nonexistent".into(),
            pattern: "parse_config".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();

    assert!(result.items.is_empty());
}

// ── Result shape ───────────────────────────────────────────────────────

#[test]
fn search_text_results_carry_symbol_record() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("HttpServer")).unwrap();
    assert!(!result.items.is_empty());

    let hit = &result.items[0];
    let sym = hit.symbol.as_ref().expect("should carry symbol");
    assert_eq!(sym.name, "HttpServer");
    assert_eq!(sym.kind, SymbolKind::Class);
    assert_eq!(hit.file_path, "src/server.rs");
    assert!(hit.score > 0.0);
    assert!(hit.score <= 1.0);
}

#[test]
fn search_text_results_include_line_number() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("parse_config")).unwrap();
    assert!(!result.items.is_empty());
    // line_number comes from start_line of the symbol.
    assert!(result.items[0].line_number >= 1);
}

#[test]
fn search_text_results_include_signature_as_line_content() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("parse_config")).unwrap();
    let hit = result
        .items
        .iter()
        .find(|m| m.symbol.as_ref().unwrap().name == "parse_config")
        .unwrap();
    assert!(hit.line_content.contains("parse_config"));
}

// ── Truncation & pagination ────────────────────────────────────────────

#[test]
fn search_text_truncation_metadata() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    // "http" appears in keywords of both HttpServer and handle_request.
    let result = svc
        .search_text(&TextQuery {
            repo_id: "repo-1".into(),
            pattern: "http".into(),
            filters: QueryFilters::default(),
            limit: 1,
            offset: 0,
        })
        .unwrap();

    assert_eq!(result.items.len(), 1);
    assert!(result.meta.truncated);
    assert!(result.meta.total_candidates >= 2);
}

#[test]
fn search_text_offset_pagination() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let full = svc.search_text(&query("http")).unwrap();

    let page2 = svc
        .search_text(&TextQuery {
            repo_id: "repo-1".into(),
            pattern: "http".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 1,
        })
        .unwrap();

    assert_eq!(page2.items.len(), full.items.len() - 1);
}

// ── Determinism ────────────────────────────────────────────────────────

#[test]
fn search_text_deterministic_ordering() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let r1 = svc.search_text(&query("http")).unwrap();
    let r2 = svc.search_text(&query("http")).unwrap();

    let ids1: Vec<&str> = r1
        .items
        .iter()
        .map(|m| m.symbol.as_ref().unwrap().id.as_str())
        .collect();
    let ids2: Vec<&str> = r2
        .items
        .iter()
        .map(|m| m.symbol.as_ref().unwrap().id.as_str())
        .collect();
    assert_eq!(ids1, ids2);
}

// ── FTS index stays in sync ────────────────────────────────────────────

#[test]
fn fts_index_updated_on_symbol_insert() {
    let store = MetadataStore::open_in_memory().unwrap();

    store
        .repos()
        .upsert(&RepoRecord {
            repo_id: "r".into(),
            display_name: "R".into(),
            source_root: "/r".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            index_version: "1.0.0".into(),
            language_counts: BTreeMap::new(),
            file_count: 1,
            symbol_count: 0,
            git_head: None,
            registered_at: Some("2026-03-09T00:00:00Z".to_string()),
            indexing_status: IndexingStatus::Ready,
            freshness_status: FreshnessStatus::Fresh,
        })
        .unwrap();

    store
        .files()
        .upsert(&FileRecord {
            repo_id: "r".into(),
            file_path: "a.rs".into(),
            language: "rust".into(),
            file_hash: "h".into(),
            summary: "s".into(),
            symbol_count: 0,
            quality_mix: QualityMix {
                semantic_percent: 0.0,
                syntax_percent: 100.0,
            },
            updated_at: "2026-03-09T00:00:00Z".into(),
        })
        .unwrap();

    let svc = StoreQueryService::new(&store);

    // Before insert: no results.
    let before = svc.search_text(&TextQuery {
        repo_id: "r".into(),
        pattern: "unique_function_name".into(),
        filters: QueryFilters::default(),
        limit: 10,
        offset: 0,
    });
    assert!(before.unwrap().items.is_empty());

    // Insert symbol.
    let qualified_name = "crate::unique_function_name".to_string();
    store
        .symbols()
        .upsert(&SymbolRecord {
            id: core_model::build_symbol_id("r", "a.rs", &qualified_name, SymbolKind::Function)
                .expect("build id"),
            repo_id: "r".into(),
            file_path: "a.rs".into(),
            language: "rust".into(),
            kind: SymbolKind::Function,
            name: "unique_function_name".into(),
            qualified_name,
            signature: "fn unique_function_name()".into(),
            start_line: 1,
            end_line: 10,
            start_byte: 0,
            byte_length: 100,
            content_hash: "hash-unique".into(),
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
        })
        .unwrap();

    // After insert: found.
    let after = svc
        .search_text(&TextQuery {
            repo_id: "r".into(),
            pattern: "unique_function_name".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(after.items.len(), 1);
    assert_eq!(
        after.items[0].symbol.as_ref().unwrap().name,
        "unique_function_name"
    );
}

#[test]
fn fts_index_updated_on_symbol_delete() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    // Confirm match exists before delete.
    let before = svc.search_text(&query("parse_config")).unwrap();
    assert!(!before.items.is_empty());

    // Delete via file cascade.
    store.files().delete("repo-1", "src/lib.rs").unwrap();

    // After delete: no more match.
    let after = svc.search_text(&query("parse_config")).unwrap();
    assert!(after.items.is_empty());
}

// ── Query normalization ───────────────────────────────────────────────

#[test]
fn search_text_malformed_fts_syntax_does_not_error() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    // FTS5 operators and special chars are normalized, not passed raw.
    // This must not produce an FTS5 syntax error.
    let result = svc.search_text(&query("\"parse_config\" OR foo*"));
    assert!(result.is_ok());

    // A query with only the target term plus stripped syntax should still match.
    let result = svc.search_text(&query("parse_config*")).unwrap();
    assert!(result
        .items
        .iter()
        .any(|m| m.symbol.as_ref().unwrap().name == "parse_config"));
}

#[test]
fn search_text_only_special_chars_returns_empty() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    let result = svc.search_text(&query("*** ---")).unwrap();
    assert!(result.items.is_empty());
}

#[test]
fn search_text_bare_fts_operator_does_not_error() {
    let store = seed_store();
    let svc = StoreQueryService::new(&store);

    // Bare FTS5 operators are lowercased to plain terms, not syntax errors.
    for op in ["OR", "AND", "NOT", "NEAR", "AND NOT"] {
        let result = svc.search_text(&query(op));
        assert!(result.is_ok(), "operator '{op}' should not cause an error");
    }
}

// ── Pipeline end-to-end ───────────────────────────────────────────────

mod pipeline {
    use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
    use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
    use indexer::{run, PipelineContext};
    use query_engine::{QueryFilters, QueryService, StoreQueryService, TextQuery};
    use store::MetadataStore;
    use tempfile::TempDir;

    struct TreeSitterRouter {
        adapters: Vec<TreeSitterAdapter>,
    }

    impl TreeSitterRouter {
        fn new() -> Self {
            let adapters = supported_languages()
                .iter()
                .filter_map(|lang| create_adapter(lang))
                .collect();
            Self { adapters }
        }
    }

    impl AdapterRouter for TreeSitterRouter {
        fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
            self.adapters
                .iter()
                .filter(|a| a.language() == language)
                .map(|a| a as &dyn LanguageAdapter)
                .collect()
        }
    }

    #[test]
    fn pipeline_indexed_symbols_searchable_via_fts() {
        // 1. Create a fixture repo with Rust source files.
        let repo_dir = TempDir::new().expect("create repo dir");
        let src = repo_dir.path().join("src");
        std::fs::create_dir_all(&src).expect("create src dir");
        std::fs::write(
            src.join("lib.rs"),
            r#"
/// Parses a TOML configuration file.
fn parse_config(path: &str) -> String {
    String::new()
}

/// An HTTP server implementation.
struct HttpServer {
    port: u16,
}
"#,
        )
        .expect("write lib.rs");

        let blob_dir = TempDir::new().expect("create blob dir");
        let blob_store =
            store::BlobStore::open(&blob_dir.path().join("blobs")).expect("open blob store");

        // 2. Index via the full pipeline.
        let mut db = MetadataStore::open_in_memory().expect("open store");
        let router = TreeSitterRouter::new();
        let ctx = PipelineContext {
            repo_id: "pipeline-test".to_string(),
            source_root: repo_dir.path().to_path_buf(),
            router: &router,
            policy_override: Some(AdapterPolicy::SyntaxOnly),
            correlation_id: None,
            use_git_diff: false,
        };
        let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
        assert!(result.metrics.symbols_extracted >= 2);

        // 3. Query via search_text and verify FTS index was populated.
        let svc = StoreQueryService::new(&db);

        let config_results = svc
            .search_text(&TextQuery {
                repo_id: "pipeline-test".into(),
                pattern: "parse_config".into(),
                filters: QueryFilters::default(),
                limit: 10,
                offset: 0,
            })
            .unwrap();
        assert!(
            !config_results.items.is_empty(),
            "parse_config should be findable via FTS after pipeline indexing"
        );
        assert!(config_results
            .items
            .iter()
            .any(|m| m.symbol.as_ref().unwrap().name == "parse_config"));

        let server_results = svc
            .search_text(&TextQuery {
                repo_id: "pipeline-test".into(),
                pattern: "HttpServer".into(),
                filters: QueryFilters::default(),
                limit: 10,
                offset: 0,
            })
            .unwrap();
        assert!(
            !server_results.items.is_empty(),
            "HttpServer should be findable via FTS after pipeline indexing"
        );
    }
}

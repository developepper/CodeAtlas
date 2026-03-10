//! Integration tests for the indexer pipeline.
//!
//! These tests exercise the full discovery → parse → persist flow against
//! real temp-dir repositories with an in-memory SQLite store and a
//! temporary blob store.

use std::path::PathBuf;

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, AdapterPolicy, AdapterRouter,
    ExtractedSymbol, IndexContext, LanguageAdapter, SourceFile, SourceSpan,
};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use core_model::{QualityLevel, SymbolKind};
use indexer::{run, stage, PipelineContext, PipelineError};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Stub adapter and router
// ---------------------------------------------------------------------------

struct StubAdapter;

impl LanguageAdapter for StubAdapter {
    fn adapter_id(&self) -> &str {
        "stub-syntax-rust"
    }

    fn language(&self) -> &str {
        "rust"
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: true,
            supports_doc_extraction: false,
        };
        &CAPS
    }

    fn index_file(
        &self,
        _ctx: &IndexContext,
        file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        if file.language != "rust" {
            return Err(AdapterError::Unsupported {
                language: file.language.clone(),
            });
        }
        Ok(AdapterOutput {
            symbols: vec![ExtractedSymbol {
                name: "main".to_string(),
                qualified_name: "main".to_string(),
                kind: SymbolKind::Function,
                span: SourceSpan {
                    start_line: 1,
                    end_line: 3,
                    start_byte: 0,
                    byte_length: 14,
                },
                signature: "fn main()".to_string(),
                confidence_score: None,
                docstring: None,
                parent_qualified_name: None,
            }],
            source_adapter: "stub-syntax-rust".to_string(),
            quality_level: QualityLevel::Syntax,
        })
    }
}

/// A policy-gating router that only returns adapters when called with the
/// expected policy. If the pipeline passes any other policy, `select`
/// returns an empty vec, causing the parse stage to record a file error
/// instead of a successful parse — making regressions observable.
struct PolicyGatingRouter {
    adapter: StubAdapter,
    expected_policy: AdapterPolicy,
}

impl PolicyGatingRouter {
    fn expecting(policy: AdapterPolicy) -> Self {
        Self {
            adapter: StubAdapter,
            expected_policy: policy,
        }
    }
}

impl AdapterRouter for PolicyGatingRouter {
    fn select(&self, language: &str, policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        if policy != self.expected_policy {
            return vec![];
        }
        if language == "rust" {
            vec![&self.adapter]
        } else {
            vec![]
        }
    }
}

struct StubRouter {
    adapter: StubAdapter,
}

impl StubRouter {
    fn new() -> Self {
        Self {
            adapter: StubAdapter,
        }
    }
}

impl AdapterRouter for StubRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        if language == "rust" {
            vec![&self.adapter]
        } else {
            vec![]
        }
    }
}

/// Router backed by real tree-sitter adapters. Returns adapters for all
/// languages supported by `adapter-syntax-treesitter`.
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_test_repo() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    dir
}

fn setup_blob_store(dir: &TempDir) -> store::BlobStore {
    store::BlobStore::open(&dir.path().join("blobs")).expect("open blob store")
}

// ---------------------------------------------------------------------------
// End-to-end pipeline smoke test (stub adapter)
// ---------------------------------------------------------------------------

#[test]
fn pipeline_end_to_end_smoke_test() {
    let repo_dir = setup_test_repo();
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = StubRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("test-correlation-001".to_string()),
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Discovery found at least the one .rs file.
    assert!(
        result.metrics.files_discovered >= 1,
        "expected at least 1 discovered file, got {}",
        result.metrics.files_discovered
    );

    // Parse processed the Rust file.
    assert_eq!(
        result.metrics.files_parsed, 1,
        "expected 1 parsed file, got {}",
        result.metrics.files_parsed
    );

    // At least one symbol was extracted.
    assert!(
        result.metrics.symbols_extracted >= 1,
        "expected at least 1 symbol, got {}",
        result.metrics.symbols_extracted
    );

    // Verify persistence: repo record exists.
    let repo = db
        .repos()
        .get("test-repo")
        .expect("query repo")
        .expect("repo record should exist");
    assert_eq!(repo.repo_id, "test-repo");
    assert!(repo.file_count >= 1);
    assert!(repo.symbol_count >= 1);

    // Verify persistence: file record exists.
    let file = db
        .files()
        .get("test-repo", "src/main.rs")
        .expect("query file")
        .expect("file record should exist");
    assert_eq!(file.language, "rust");
    assert!(file.symbol_count >= 1);

    // Verify persistence: symbol record exists.
    let symbol_id = core_model::build_symbol_id("src/main.rs", "main", SymbolKind::Function)
        .expect("build symbol id");
    let symbol = db
        .symbols()
        .get(&symbol_id)
        .expect("query symbol")
        .expect("symbol record should exist");
    assert_eq!(symbol.name, "main");
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(symbol.repo_id, "test-repo");

    // Verify blob was written and is retrievable by content hash.
    let content = std::fs::read(repo_dir.path().join("src/main.rs")).expect("read source");
    let hash = store::content_hash(&content);
    assert!(blob_store.exists(&hash).expect("blob exists check"));
    let blob = blob_store
        .get(&hash)
        .expect("get blob")
        .expect("blob present");
    assert_eq!(blob, content);
}

// ---------------------------------------------------------------------------
// End-to-end with real tree-sitter adapter
// ---------------------------------------------------------------------------

#[test]
fn pipeline_end_to_end_with_treesitter_adapter() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Write a non-trivial Rust file with multiple symbol types.
    std::fs::write(
        src.join("lib.rs"),
        r#"/// A configuration holder.
pub struct Config {
    pub name: String,
}

impl Config {
    /// Creates a new Config.
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

/// Top-level helper function.
pub fn greet(config: &Config) -> String {
    format!("Hello, {}!", config.name)
}
"#,
    )
    .expect("write lib.rs");

    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "treesitter-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Both .rs files were parsed.
    assert_eq!(result.metrics.files_parsed, 2);
    assert!(result.file_errors.is_empty(), "no file errors expected");

    // Verify repo aggregates.
    let repo = db
        .repos()
        .get("treesitter-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 2);
    assert!(
        repo.symbol_count >= 3,
        "expected at least Config, new, greet"
    );

    // Verify lib.rs file record.
    let lib_file = db
        .files()
        .get("treesitter-repo", "src/lib.rs")
        .expect("query file")
        .expect("lib.rs record");
    assert_eq!(lib_file.language, "rust");
    assert!(lib_file.symbol_count >= 3);

    // Verify symbol records have correct provenance.
    let symbol_ids = db
        .symbols()
        .list_ids_for_file("treesitter-repo", "src/lib.rs")
        .expect("list symbols");
    assert!(symbol_ids.len() >= 3);

    // Verify source_adapter provenance on a symbol.
    let first_sym = db
        .symbols()
        .get(&symbol_ids[0])
        .expect("get symbol")
        .expect("symbol exists");
    assert!(
        first_sym.source_adapter.contains("syntax-treesitter"),
        "source_adapter should identify tree-sitter: {}",
        first_sym.source_adapter
    );
    assert_eq!(first_sym.quality_level, QualityLevel::Syntax);

    // Verify blobs were written for both files.
    let lib_content = std::fs::read(src.join("lib.rs")).expect("read lib.rs");
    let lib_hash = store::content_hash(&lib_content);
    assert!(blob_store.exists(&lib_hash).expect("blob exists"));

    let main_content = std::fs::read(src.join("main.rs")).expect("read main.rs");
    let main_hash = store::content_hash(&main_content);
    assert!(blob_store.exists(&main_hash).expect("blob exists"));
}

#[test]
fn pipeline_treesitter_unsupported_language_reports_error_with_provenance() {
    let repo_dir = TempDir::new().expect("create temp dir");

    // Write a Rust file (supported) and a Python file (unsupported by tree-sitter router).
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write rs");
    std::fs::write(repo_dir.path().join("script.py"), "print('hi')\n").expect("write py");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "mixed-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Rust file parsed successfully.
    assert_eq!(result.metrics.files_parsed, 1);

    // Python file recorded as an error (no adapter available).
    let py_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        !py_errors.is_empty(),
        "expected error for unsupported Python file"
    );
    assert!(
        py_errors[0].error.contains("no adapter"),
        "error should indicate no adapter: {}",
        py_errors[0].error
    );
    // No adapter was tried, so adapter_id should be None.
    assert!(
        py_errors[0].adapter_id.is_none(),
        "adapter_id should be None when no adapters available"
    );

    // Repo still persisted with the successful file.
    let repo = db
        .repos()
        .get("mixed-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 1);
}

// ---------------------------------------------------------------------------
// Adapter fallback and error provenance
// ---------------------------------------------------------------------------

/// An adapter that always fails with an Internal error.
struct FailingAdapter;

impl LanguageAdapter for FailingAdapter {
    fn adapter_id(&self) -> &str {
        "failing-adapter"
    }

    fn language(&self) -> &str {
        "rust"
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: false,
            supports_doc_extraction: false,
        };
        &CAPS
    }

    fn index_file(
        &self,
        _ctx: &IndexContext,
        _file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        Err(AdapterError::Parse {
            path: _file.relative_path.clone(),
            reason: "simulated failure".to_string(),
        })
    }
}

/// Router that returns a failing adapter first, then the stub adapter.
/// Verifies the pipeline falls through to the second adapter on error.
struct FallbackRouter {
    failing: FailingAdapter,
    fallback: StubAdapter,
}

impl FallbackRouter {
    fn new() -> Self {
        Self {
            failing: FailingAdapter,
            fallback: StubAdapter,
        }
    }
}

impl AdapterRouter for FallbackRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        if language == "rust" {
            vec![&self.failing, &self.fallback]
        } else {
            vec![]
        }
    }
}

#[test]
fn adapter_fallback_continues_past_failing_adapter() {
    let repo_dir = setup_test_repo();
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = FallbackRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "fallback-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // The file should have been parsed by the fallback adapter.
    assert_eq!(
        result.metrics.files_parsed, 1,
        "fallback adapter should have succeeded"
    );

    // No file errors — the failing adapter's error should not prevent success.
    assert!(
        result.file_errors.is_empty(),
        "file should have been parsed by fallback adapter, got errors: {:?}",
        result.file_errors
    );

    // Verify the symbol was persisted.
    let repo = db
        .repos()
        .get("fallback-repo")
        .expect("query repo")
        .expect("repo record");
    assert!(repo.symbol_count >= 1);
}

/// Router that only returns the failing adapter — no fallback.
struct FailOnlyRouter {
    failing: FailingAdapter,
}

impl FailOnlyRouter {
    fn new() -> Self {
        Self {
            failing: FailingAdapter,
        }
    }
}

impl AdapterRouter for FailOnlyRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        if language == "rust" {
            vec![&self.failing]
        } else {
            vec![]
        }
    }
}

#[test]
fn adapter_error_carries_adapter_id_provenance() {
    let repo_dir = setup_test_repo();
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = FailOnlyRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "fail-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // No files parsed.
    assert_eq!(result.metrics.files_parsed, 0);

    // Error carries the adapter ID for provenance.
    let rs_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("main.rs"))
        .collect();
    assert!(!rs_errors.is_empty(), "expected error for main.rs");
    assert_eq!(
        rs_errors[0].adapter_id.as_deref(),
        Some("failing-adapter"),
        "error should carry the failing adapter's ID"
    );
    assert!(
        rs_errors[0].error.contains("simulated failure"),
        "error message should contain adapter error: {}",
        rs_errors[0].error
    );
}

// ---------------------------------------------------------------------------
// Discovery stage tests
// ---------------------------------------------------------------------------

#[test]
fn discovery_stage_finds_files() {
    let repo_dir = setup_test_repo();
    let router = StubRouter::new();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let output = stage::discover(&ctx).expect("discovery should succeed");
    assert!(!output.files.is_empty());
    assert!(output.files.iter().any(|f| f.language == "rust"));
}

#[test]
fn discovery_stage_rejects_invalid_root() {
    let router = StubRouter::new();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: PathBuf::from("/nonexistent/path"),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let err = stage::discover(&ctx).expect_err("should fail on invalid root");
    assert!(matches!(err, PipelineError::Discovery(_)));
}

#[test]
fn discovery_detects_extensionless_script_via_content() {
    let dir = TempDir::new().expect("create temp dir");
    std::fs::write(
        dir.path().join("run-server"),
        "#!/usr/bin/env python\nprint('hi')\n",
    )
    .expect("write script");

    let router = StubRouter::new();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let output = stage::discover(&ctx).expect("discovery should succeed");
    let script = output
        .files
        .iter()
        .find(|f| f.relative_path.to_string_lossy().contains("run-server"));
    assert!(
        script.is_some(),
        "extensionless script should be discovered"
    );
    assert_eq!(script.unwrap().language, "python");
}

// ---------------------------------------------------------------------------
// Parse stage tests
// ---------------------------------------------------------------------------

#[test]
fn parse_stage_handles_no_adapter() {
    let repo_dir = setup_test_repo();
    // Write a Python file that the stub router won't handle.
    std::fs::write(repo_dir.path().join("script.py"), "print('hi')\n").expect("write py file");

    let router = StubRouter::new();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let discovery = stage::discover(&ctx).expect("discovery ok");
    let parse_output = stage::parse(&ctx, &discovery);

    // The Python file should appear in file_errors.
    let py_errors: Vec<_> = parse_output
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        !py_errors.is_empty(),
        "expected error for unsupported Python file"
    );
}

#[test]
fn parse_stage_uses_context_default_policy() {
    let repo_dir = setup_test_repo();

    // Use a policy-gating router that only returns adapters when called
    // with SemanticPreferred. If the pipeline regresses to hard-coding
    // SyntaxOnly (the global default for Rust), the router returns no
    // adapters and the file lands in file_errors instead of parsed_files.
    let router = PolicyGatingRouter::expecting(AdapterPolicy::SemanticPreferred);

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SemanticPreferred,
        correlation_id: None,
    };

    let discovery = stage::discover(&ctx).expect("discovery ok");
    let parse_output = stage::parse(&ctx, &discovery);

    // Parsing succeeds only if the router received SemanticPreferred.
    assert_eq!(
        parse_output.parsed_files.len(),
        1,
        "expected 1 parsed file — router should have received SemanticPreferred from context"
    );
    assert!(
        parse_output.file_errors.is_empty(),
        "no file errors expected when correct policy is forwarded"
    );
}

// ---------------------------------------------------------------------------
// Re-index aggregate consistency
// ---------------------------------------------------------------------------

#[test]
fn reindex_removes_stale_files_and_recomputes_aggregates() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Initial index: two Rust files.
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(src.join("lib.rs"), "pub fn greet() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "aggregate-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run: both files indexed.
    let r1 = run(&ctx, &mut db, &blob_store).expect("first run");
    assert_eq!(r1.metrics.files_parsed, 2);

    let repo = db
        .repos()
        .get("aggregate-repo")
        .expect("query repo")
        .expect("repo exists");
    assert_eq!(repo.file_count, 2);
    let first_symbol_count = repo.symbol_count;
    assert!(first_symbol_count >= 2, "at least main + greet");

    // Remove lib.rs and re-index.
    std::fs::remove_file(src.join("lib.rs")).expect("remove lib.rs");
    let r2 = run(&ctx, &mut db, &blob_store).expect("second run");
    // main.rs is unchanged, so incremental indexing skips it.
    assert_eq!(r2.metrics.files_parsed, 0);
    assert_eq!(r2.metrics.files_unchanged, 1);
    assert_eq!(r2.metrics.files_deleted, 1);

    // Verify stale file was removed.
    assert!(
        db.files()
            .get("aggregate-repo", "src/lib.rs")
            .expect("query")
            .is_none(),
        "stale file src/lib.rs should have been removed"
    );

    // Verify stale symbols were removed.
    let lib_symbols = db
        .symbols()
        .list_ids_for_file("aggregate-repo", "src/lib.rs")
        .expect("query");
    assert!(
        lib_symbols.is_empty(),
        "symbols for removed file should be gone"
    );

    // Verify repo aggregates were recomputed from DB state.
    let repo = db
        .repos()
        .get("aggregate-repo")
        .expect("query repo")
        .expect("repo exists");
    assert_eq!(repo.file_count, 1, "file_count should reflect only main.rs");
    assert!(
        repo.symbol_count < first_symbol_count,
        "symbol_count should have decreased after removing lib.rs"
    );
    assert_eq!(
        repo.language_counts.get("rust"),
        Some(&1),
        "language_counts should reflect 1 rust file"
    );

    // Remaining file should still be present and correct.
    let main_file = db
        .files()
        .get("aggregate-repo", "src/main.rs")
        .expect("query")
        .expect("main.rs should still exist");
    assert_eq!(main_file.language, "rust");
}

#[test]
fn reindex_cleans_up_removed_symbols_within_file() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Initial content: two functions.
    std::fs::write(src.join("lib.rs"), "pub fn alpha() {}\npub fn beta() {}\n")
        .expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "symbol-cleanup-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run.
    run(&ctx, &mut db, &blob_store).expect("first run");
    let syms = db
        .symbols()
        .list_ids_for_file("symbol-cleanup-repo", "src/lib.rs")
        .expect("list symbols");
    assert!(syms.len() >= 2, "expected at least alpha + beta");

    // Remove beta, keep alpha.
    std::fs::write(src.join("lib.rs"), "pub fn alpha() {}\n").expect("rewrite lib.rs");

    // Second run.
    run(&ctx, &mut db, &blob_store).expect("second run");
    let syms = db
        .symbols()
        .list_ids_for_file("symbol-cleanup-repo", "src/lib.rs")
        .expect("list symbols");

    // Only alpha should remain.
    assert_eq!(syms.len(), 1, "only alpha should remain, got: {syms:?}");
    assert!(
        syms[0].contains("alpha"),
        "remaining symbol should be alpha: {}",
        syms[0]
    );

    // Repo aggregate should reflect the reduced count.
    let repo = db
        .repos()
        .get("symbol-cleanup-repo")
        .expect("query repo")
        .expect("repo exists");
    assert_eq!(repo.file_count, 1);
    assert_eq!(repo.symbol_count, 1);
}

// ---------------------------------------------------------------------------
// Enrichment field persistence
// ---------------------------------------------------------------------------

#[test]
fn enrichment_fields_persisted_for_files_and_symbols() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    std::fs::write(
        src.join("lib.rs"),
        r#"/// A configuration holder.
pub struct Config {
    pub name: String,
}

impl Config {
    /// Creates a new Config.
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

pub fn greet(config: &Config) -> String {
    format!("Hello, {}!", config.name)
}
"#,
    )
    .expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "enrich-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
    assert!(result.metrics.files_parsed >= 1);

    // Verify file summary is heuristic (not the old hardcoded placeholder).
    let file = db
        .files()
        .get("enrich-repo", "src/lib.rs")
        .expect("query file")
        .expect("file record exists");
    assert!(
        file.summary.contains("function") || file.summary.contains("class"),
        "file summary should describe symbol kinds: {}",
        file.summary
    );
    assert!(
        !file.summary.starts_with("rust source file\n"),
        "file summary should not be the old placeholder"
    );

    // Verify symbol summaries and keywords are populated.
    let symbol_ids = db
        .symbols()
        .list_ids_for_file("enrich-repo", "src/lib.rs")
        .expect("list symbols");
    assert!(symbol_ids.len() >= 3, "expected Config, new, greet");

    let mut has_docstring_summary = false;
    let mut has_keywords = false;

    for id in &symbol_ids {
        let sym = db
            .symbols()
            .get(id)
            .expect("get symbol")
            .expect("symbol exists");

        // Every symbol should have a summary.
        assert!(
            sym.summary.is_some(),
            "symbol {} should have a summary",
            sym.name
        );

        // Symbols with docstrings should use the docstring as summary.
        if sym.docstring.is_some() {
            let summary = sym.summary.as_ref().unwrap();
            // Should be based on docstring first sentence, not signature.
            assert!(
                !summary.starts_with("Function ") && !summary.starts_with("Class "),
                "symbol {} with docstring should use docstring summary, got: {}",
                sym.name,
                summary
            );
            has_docstring_summary = true;
        }

        // At least some symbols should have keywords.
        if let Some(kw) = &sym.keywords {
            assert!(!kw.is_empty(), "keywords should not be empty");
            // Keywords should be sorted and deduplicated.
            let mut sorted = kw.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(kw, &sorted, "keywords should be sorted and deduplicated");
            has_keywords = true;
        }
    }

    assert!(
        has_docstring_summary,
        "at least one symbol should have a docstring-based summary"
    );
    assert!(
        has_keywords,
        "at least one symbol should have extracted keywords"
    );
}

#[test]
fn enrichment_is_deterministic_across_runs() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(
        repo_dir.path().join("main.rs"),
        "fn main() {}\npub fn helper() {}\n",
    )
    .expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();

    // Run pipeline twice with separate DBs.
    let mut db1 = store::MetadataStore::open_in_memory().expect("open store");
    let mut db2 = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "det-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    run(&ctx, &mut db1, &blob_store).expect("run 1");
    run(&ctx, &mut db2, &blob_store).expect("run 2");

    // Compare file summaries.
    let f1 = db1
        .files()
        .get("det-repo", "main.rs")
        .unwrap()
        .expect("file 1");
    let f2 = db2
        .files()
        .get("det-repo", "main.rs")
        .unwrap()
        .expect("file 2");
    assert_eq!(
        f1.summary, f2.summary,
        "file summary should be deterministic"
    );

    // Compare symbol summaries and keywords.
    let ids1 = db1
        .symbols()
        .list_ids_for_file("det-repo", "main.rs")
        .unwrap();
    let ids2 = db2
        .symbols()
        .list_ids_for_file("det-repo", "main.rs")
        .unwrap();
    assert_eq!(ids1, ids2);

    for (id1, id2) in ids1.iter().zip(ids2.iter()) {
        let s1 = db1.symbols().get(id1).unwrap().expect("sym 1");
        let s2 = db2.symbols().get(id2).unwrap().expect("sym 2");
        assert_eq!(
            s1.summary, s2.summary,
            "symbol summary should be deterministic"
        );
        assert_eq!(s1.keywords, s2.keywords, "keywords should be deterministic");
    }
}

// ---------------------------------------------------------------------------
// Adapter failure isolation
// ---------------------------------------------------------------------------

/// An adapter that fails only for files whose path contains a substring.
struct PathSelectiveFailAdapter {
    fail_substring: &'static str,
}

impl LanguageAdapter for PathSelectiveFailAdapter {
    fn adapter_id(&self) -> &str {
        "path-selective-fail"
    }

    fn language(&self) -> &str {
        "rust"
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: false,
            supports_doc_extraction: false,
        };
        &CAPS
    }

    fn index_file(
        &self,
        _ctx: &IndexContext,
        file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        if file
            .relative_path
            .to_string_lossy()
            .contains(self.fail_substring)
        {
            return Err(AdapterError::Parse {
                path: file.relative_path.clone(),
                reason: "simulated selective failure".to_string(),
            });
        }
        Ok(AdapterOutput {
            symbols: vec![ExtractedSymbol {
                name: "stub_fn".to_string(),
                qualified_name: "stub_fn".to_string(),
                kind: SymbolKind::Function,
                span: SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    start_byte: 0,
                    byte_length: 10,
                },
                signature: "fn stub_fn()".to_string(),
                confidence_score: None,
                docstring: None,
                parent_qualified_name: None,
            }],
            source_adapter: "path-selective-fail".to_string(),
            quality_level: QualityLevel::Syntax,
        })
    }
}

/// Router that uses PathSelectiveFailAdapter as the only adapter.
struct SelectiveFailOnlyRouter {
    adapter: PathSelectiveFailAdapter,
}

impl SelectiveFailOnlyRouter {
    fn failing_on(substring: &'static str) -> Self {
        Self {
            adapter: PathSelectiveFailAdapter {
                fail_substring: substring,
            },
        }
    }
}

impl AdapterRouter for SelectiveFailOnlyRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        if language == "rust" {
            vec![&self.adapter]
        } else {
            vec![]
        }
    }
}

#[test]
fn reindex_with_adapter_failure_preserves_previously_indexed_file() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(src.join("lib.rs"), "pub fn helper() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    // First run: use the tree-sitter router — both files index successfully.
    let ts_router = TreeSitterRouter::new();
    let ctx1 = PipelineContext {
        repo_id: "fail-isolation-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &ts_router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let r1 = run(&ctx1, &mut db, &blob_store).expect("first run");
    assert_eq!(r1.metrics.files_parsed, 2);

    // Verify both files are in the DB.
    assert!(
        db.files()
            .get("fail-isolation-repo", "src/main.rs")
            .unwrap()
            .is_some(),
        "main.rs should be indexed"
    );
    assert!(
        db.files()
            .get("fail-isolation-repo", "src/lib.rs")
            .unwrap()
            .is_some(),
        "lib.rs should be indexed"
    );
    let lib_symbols_before = db
        .symbols()
        .list_ids_for_file("fail-isolation-repo", "src/lib.rs")
        .unwrap();
    assert!(!lib_symbols_before.is_empty(), "lib.rs should have symbols");

    // Second run: use a router that fails on lib.rs.
    let fail_router = SelectiveFailOnlyRouter::failing_on("lib.rs");
    let ctx2 = PipelineContext {
        repo_id: "fail-isolation-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &fail_router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    let r2 = run(&ctx2, &mut db, &blob_store).expect("second run");
    // Both files are unchanged from the first run, so incremental indexing
    // skips them entirely — the failing adapter is never invoked.
    assert_eq!(r2.metrics.files_parsed, 0);
    assert_eq!(r2.metrics.files_unchanged, 2);

    // lib.rs file record should still exist (not purged by stale cleanup).
    assert!(
        db.files()
            .get("fail-isolation-repo", "src/lib.rs")
            .unwrap()
            .is_some(),
        "lib.rs metadata should be preserved despite adapter failure"
    );

    // lib.rs symbols from the first run should still be present.
    let lib_symbols_after = db
        .symbols()
        .list_ids_for_file("fail-isolation-repo", "src/lib.rs")
        .unwrap();
    assert_eq!(
        lib_symbols_before, lib_symbols_after,
        "lib.rs symbols should be unchanged after failed re-index"
    );

    // main.rs should still be present and updated.
    assert!(
        db.files()
            .get("fail-isolation-repo", "src/main.rs")
            .unwrap()
            .is_some(),
        "main.rs should still be indexed"
    );
}

// ---------------------------------------------------------------------------
// Error display coverage
// ---------------------------------------------------------------------------

#[test]
fn pipeline_error_display_covers_all_variants() {
    let disc = PipelineError::Discovery(repo_walker::WalkError::InvalidRoot {
        path: PathBuf::from("/bad"),
        reason: "not found",
    });
    assert!(disc.to_string().contains("discovery error"));

    let io = PipelineError::Io {
        path: Some(PathBuf::from("foo.rs")),
        source: std::io::Error::other("disk full"),
    };
    assert!(io.to_string().contains("I/O error"));

    let io_no_path = PipelineError::Io {
        path: None,
        source: std::io::Error::other("disk full"),
    };
    assert!(io_no_path.to_string().contains("I/O error"));

    let persist = PipelineError::Persist(store::StoreError::Validation("bad".to_string()));
    assert!(persist.to_string().contains("persist error"));

    let validation = PipelineError::Validation("bad field".to_string());
    assert!(validation.to_string().contains("validation error"));

    let internal = PipelineError::Internal("oops".to_string());
    assert!(internal.to_string().contains("internal error"));
}

// ---------------------------------------------------------------------------
// Incremental indexing: file hash map and changed-file detection
// ---------------------------------------------------------------------------

#[test]
fn incremental_noop_run_skips_all_files() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run: full index.
    let r1 = run(&ctx, &mut db, &blob_store).expect("first run");
    assert_eq!(r1.metrics.files_parsed, 1);
    assert_eq!(r1.metrics.files_unchanged, 0);
    assert_eq!(r1.metrics.files_deleted, 0);

    // Second run: no changes — should be a no-op.
    let r2 = run(&ctx, &mut db, &blob_store).expect("second run");
    assert_eq!(r2.metrics.files_parsed, 0, "no files should be re-parsed");
    assert_eq!(r2.metrics.files_unchanged, 1);
    assert_eq!(r2.metrics.files_deleted, 0);
    assert_eq!(r2.metrics.symbols_extracted, 0);

    // Metadata should still be intact from the first run.
    let repo = db
        .repos()
        .get("incr-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 1);
    assert!(repo.symbol_count >= 1);

    let file = db
        .files()
        .get("incr-repo", "main.rs")
        .expect("query file")
        .expect("file record");
    assert_eq!(file.language, "rust");
    assert!(file.symbol_count >= 1);
}

#[test]
fn incremental_detects_modified_file_and_reindexes() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("lib.rs"), "pub fn alpha() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-mod-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run.
    run(&ctx, &mut db, &blob_store).expect("first run");
    let syms_before = db
        .symbols()
        .list_ids_for_file("incr-mod-repo", "lib.rs")
        .expect("list symbols");
    assert_eq!(syms_before.len(), 1);

    // Modify the file: add a second function.
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("rewrite lib.rs");

    // Second run: should detect the change and re-index.
    let r2 = run(&ctx, &mut db, &blob_store).expect("second run");
    assert_eq!(
        r2.metrics.files_parsed, 1,
        "modified file should be re-parsed"
    );
    assert_eq!(r2.metrics.files_unchanged, 0);

    // Verify the new symbol was added.
    let syms_after = db
        .symbols()
        .list_ids_for_file("incr-mod-repo", "lib.rs")
        .expect("list symbols");
    assert_eq!(syms_after.len(), 2, "should now have alpha + beta");
}

#[test]
fn incremental_detects_new_file() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-new-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run.
    run(&ctx, &mut db, &blob_store).expect("first run");

    // Add a new file.
    std::fs::write(repo_dir.path().join("lib.rs"), "pub fn helper() {}\n").expect("write lib.rs");

    // Second run: main.rs unchanged, lib.rs new.
    let r2 = run(&ctx, &mut db, &blob_store).expect("second run");
    assert_eq!(r2.metrics.files_parsed, 1, "only new file should be parsed");
    assert_eq!(r2.metrics.files_unchanged, 1, "main.rs unchanged");
    assert_eq!(r2.metrics.files_deleted, 0);

    // Both files should be in the store.
    let repo = db
        .repos()
        .get("incr-new-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 2);

    assert!(
        db.files().get("incr-new-repo", "lib.rs").unwrap().is_some(),
        "new file should be indexed"
    );
    assert!(
        db.files()
            .get("incr-new-repo", "main.rs")
            .unwrap()
            .is_some(),
        "unchanged file should still be present"
    );
}

#[test]
fn incremental_hash_map_persists_across_runs() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(repo_dir.path().join("lib.rs"), "pub fn foo() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "hash-persist-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // First run populates the hash map.
    run(&ctx, &mut db, &blob_store).expect("first run");

    let hash_map = db
        .files()
        .list_hash_map("hash-persist-repo")
        .expect("list hash map");
    assert_eq!(
        hash_map.len(),
        2,
        "hash map should have entries for both files"
    );
    assert!(hash_map.contains_key("main.rs"));
    assert!(hash_map.contains_key("lib.rs"));

    // Verify hashes match content_hash of actual file content.
    let main_content = std::fs::read(repo_dir.path().join("main.rs")).unwrap();
    let expected_hash = store::content_hash(&main_content);
    assert_eq!(hash_map["main.rs"], expected_hash);

    // Third run with no changes should still find the same hash map.
    run(&ctx, &mut db, &blob_store).expect("second run");
    let hash_map2 = db
        .files()
        .list_hash_map("hash-persist-repo")
        .expect("list hash map");
    assert_eq!(
        hash_map, hash_map2,
        "hash map should be stable across no-op runs"
    );
}

// ---------------------------------------------------------------------------
// Incremental reindex: deleted-file cleanup and lifecycle
// ---------------------------------------------------------------------------

#[test]
fn incremental_deleted_file_removes_symbols_and_updates_aggregates() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "del-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1: index both files.
    let r1 = run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(r1.metrics.files_parsed, 2);
    assert_eq!(r1.metrics.files_deleted, 0);

    let repo = db.repos().get("del-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 2);
    let initial_symbol_count = repo.symbol_count;
    assert!(initial_symbol_count >= 3, "main + alpha + beta");

    // Verify lib.rs symbols exist.
    let lib_syms = db
        .symbols()
        .list_ids_for_file("del-repo", "lib.rs")
        .unwrap();
    assert!(lib_syms.len() >= 2, "alpha + beta");

    // Delete lib.rs from disk.
    std::fs::remove_file(repo_dir.path().join("lib.rs")).expect("remove lib.rs");

    // Run 2: lib.rs deleted, main.rs unchanged.
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_parsed, 0, "main.rs unchanged");
    assert_eq!(r2.metrics.files_unchanged, 1);
    assert_eq!(r2.metrics.files_deleted, 1);

    // lib.rs file record should be gone.
    assert!(
        db.files().get("del-repo", "lib.rs").unwrap().is_none(),
        "deleted file record should be removed"
    );

    // lib.rs symbols should be gone (cascade delete).
    let lib_syms_after = db
        .symbols()
        .list_ids_for_file("del-repo", "lib.rs")
        .unwrap();
    assert!(
        lib_syms_after.is_empty(),
        "symbols for deleted file should be removed"
    );

    // Hash map should no longer contain lib.rs.
    let hash_map = db.files().list_hash_map("del-repo").unwrap();
    assert!(!hash_map.contains_key("lib.rs"));
    assert!(hash_map.contains_key("main.rs"));

    // Aggregates should reflect the deletion.
    let repo = db.repos().get("del-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 1);
    assert!(
        repo.symbol_count < initial_symbol_count,
        "symbol count should decrease"
    );
    assert_eq!(repo.language_counts.get("rust"), Some(&1));
}

#[test]
fn incremental_full_lifecycle_add_modify_delete() {
    // This test exercises the complete incremental lifecycle over 5 runs:
    // Run 1: Initial index (2 files)
    // Run 2: No-op (nothing changed)
    // Run 3: Add new file + modify existing file
    // Run 4: Delete one file
    // Run 5: No-op after deletion

    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(repo_dir.path().join("lib.rs"), "pub fn greet() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "lifecycle-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // --- Run 1: Initial index ---
    let r1 = run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(r1.metrics.files_parsed, 2);
    assert_eq!(r1.metrics.files_unchanged, 0);
    assert_eq!(r1.metrics.files_deleted, 0);

    let repo = db.repos().get("lifecycle-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 2);
    let run1_symbol_count = repo.symbol_count;

    // --- Run 2: No-op ---
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_parsed, 0);
    assert_eq!(r2.metrics.files_unchanged, 2);
    assert_eq!(r2.metrics.files_deleted, 0);
    assert_eq!(r2.metrics.symbols_extracted, 0);

    // Aggregates unchanged.
    let repo = db.repos().get("lifecycle-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 2);
    assert_eq!(repo.symbol_count, run1_symbol_count);

    // --- Run 3: Add utils.rs + modify lib.rs ---
    std::fs::write(repo_dir.path().join("utils.rs"), "pub fn helper() {}\n")
        .expect("write utils.rs");
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        "pub fn greet() {}\npub fn farewell() {}\n",
    )
    .expect("rewrite lib.rs");

    let r3 = run(&ctx, &mut db, &blob_store).expect("run 3");
    assert_eq!(
        r3.metrics.files_parsed, 2,
        "utils.rs (new) + lib.rs (modified)"
    );
    assert_eq!(r3.metrics.files_unchanged, 1, "main.rs unchanged");
    assert_eq!(r3.metrics.files_deleted, 0);

    let repo = db.repos().get("lifecycle-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 3);
    assert!(
        repo.symbol_count > run1_symbol_count,
        "more symbols after adding utils.rs and modifying lib.rs"
    );
    let run3_symbol_count = repo.symbol_count;

    // Verify utils.rs was indexed.
    assert!(
        db.files()
            .get("lifecycle-repo", "utils.rs")
            .unwrap()
            .is_some(),
        "new file should be indexed"
    );

    // Verify lib.rs now has farewell.
    let lib_syms = db
        .symbols()
        .list_ids_for_file("lifecycle-repo", "lib.rs")
        .unwrap();
    assert!(
        lib_syms.len() >= 2,
        "lib.rs should have greet + farewell, got {}",
        lib_syms.len()
    );

    // --- Run 4: Delete utils.rs ---
    std::fs::remove_file(repo_dir.path().join("utils.rs")).expect("remove utils.rs");

    let r4 = run(&ctx, &mut db, &blob_store).expect("run 4");
    assert_eq!(r4.metrics.files_parsed, 0, "no changed files");
    assert_eq!(r4.metrics.files_unchanged, 2, "main.rs + lib.rs");
    assert_eq!(r4.metrics.files_deleted, 1, "utils.rs deleted");

    let repo = db.repos().get("lifecycle-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 2);
    assert!(
        repo.symbol_count < run3_symbol_count,
        "fewer symbols after deleting utils.rs"
    );

    // utils.rs gone from store.
    assert!(
        db.files()
            .get("lifecycle-repo", "utils.rs")
            .unwrap()
            .is_none(),
        "deleted file should be removed"
    );
    assert!(
        db.symbols()
            .list_ids_for_file("lifecycle-repo", "utils.rs")
            .unwrap()
            .is_empty(),
        "symbols for deleted file should be removed"
    );

    // --- Run 5: No-op after deletion ---
    let r5 = run(&ctx, &mut db, &blob_store).expect("run 5");
    assert_eq!(r5.metrics.files_parsed, 0);
    assert_eq!(r5.metrics.files_unchanged, 2);
    assert_eq!(r5.metrics.files_deleted, 0);

    // Final state is stable.
    let repo = db.repos().get("lifecycle-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 2);
    let hash_map = db.files().list_hash_map("lifecycle-repo").unwrap();
    assert_eq!(hash_map.len(), 2);
    assert!(hash_map.contains_key("main.rs"));
    assert!(hash_map.contains_key("lib.rs"));
    assert!(!hash_map.contains_key("utils.rs"));
}

#[test]
fn incremental_delete_all_files_leaves_empty_repo() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "empty-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1: index.
    run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(db.repos().get("empty-repo").unwrap().unwrap().file_count, 1);

    // Remove the only file.
    std::fs::remove_file(repo_dir.path().join("main.rs")).expect("remove main.rs");

    // Run 2: all files deleted.
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_deleted, 1);
    assert_eq!(r2.metrics.files_parsed, 0);
    assert_eq!(r2.metrics.files_unchanged, 0);

    let repo = db.repos().get("empty-repo").unwrap().unwrap();
    assert_eq!(repo.file_count, 0);
    assert_eq!(repo.symbol_count, 0);
    assert!(repo.language_counts.is_empty());

    let hash_map = db.files().list_hash_map("empty-repo").unwrap();
    assert!(hash_map.is_empty());
}

#[test]
fn incremental_multiple_deletes_across_runs() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("a.rs"), "pub fn a() {}\n").expect("write a.rs");
    std::fs::write(repo_dir.path().join("b.rs"), "pub fn b() {}\n").expect("write b.rs");
    std::fs::write(repo_dir.path().join("c.rs"), "pub fn c() {}\n").expect("write c.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "multi-del-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1: index all 3.
    let r1 = run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(r1.metrics.files_parsed, 3);

    // Delete a.rs.
    std::fs::remove_file(repo_dir.path().join("a.rs")).expect("remove a.rs");
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_deleted, 1);
    assert_eq!(
        db.repos()
            .get("multi-del-repo")
            .unwrap()
            .unwrap()
            .file_count,
        2
    );

    // Delete b.rs.
    std::fs::remove_file(repo_dir.path().join("b.rs")).expect("remove b.rs");
    let r3 = run(&ctx, &mut db, &blob_store).expect("run 3");
    assert_eq!(r3.metrics.files_deleted, 1);
    assert_eq!(
        db.repos()
            .get("multi-del-repo")
            .unwrap()
            .unwrap()
            .file_count,
        1
    );

    // c.rs should still be present and correct.
    let c_file = db
        .files()
        .get("multi-del-repo", "c.rs")
        .unwrap()
        .expect("c.rs should exist");
    assert_eq!(c_file.language, "rust");
    let c_syms = db
        .symbols()
        .list_ids_for_file("multi-del-repo", "c.rs")
        .unwrap();
    assert!(!c_syms.is_empty(), "c.rs should have symbols");
}

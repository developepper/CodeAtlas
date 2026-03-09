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

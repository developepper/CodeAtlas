//! Integration tests for the indexer pipeline.
//!
//! These tests exercise the full discovery → extract → persist flow against
//! real temp-dir repositories with an in-memory SQLite store and a
//! temporary blob store.

use std::path::PathBuf;

use core_model::{BackendId, SymbolKind};
use indexer::{
    run, stage, DefaultBackendRegistry, DispatchContext, PipelineContext, PipelineError,
    SyntaxPolicy,
};
use syntax_platform::{
    GoSyntaxBackend, PhpSyntaxBackend, PreparedFile, PythonSyntaxBackend, RustSyntaxBackend,
    SyntaxBackend, SyntaxCapability, SyntaxError, SyntaxExtraction, SyntaxSymbol,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Registry helpers
// ---------------------------------------------------------------------------

/// Registry with the real Rust syntax backend.
fn make_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    let rust_backend = RustSyntaxBackend::new();
    let rust_id = RustSyntaxBackend::backend_id();
    registry.register_syntax(rust_id, Box::new(rust_backend));
    registry
}

// ---------------------------------------------------------------------------
// Failing syntax backend (for error provenance tests)
// ---------------------------------------------------------------------------

/// A syntax backend that always fails with a Parse error.
struct FailingSyntaxBackend;

impl SyntaxBackend for FailingSyntaxBackend {
    fn language(&self) -> &str {
        "rust"
    }

    fn capability(&self) -> &SyntaxCapability {
        static CAP: SyntaxCapability = SyntaxCapability {
            supported_kinds: vec![],
            supports_containers: false,
            supports_docs: false,
        };
        &CAP
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        Err(SyntaxError::Parse {
            path: file.relative_path.clone(),
            reason: "simulated failure".to_string(),
        })
    }
}

impl FailingSyntaxBackend {
    fn backend_id() -> BackendId {
        BackendId("failing-backend".to_string())
    }
}

/// A syntax backend that fails only for files whose path contains a substring.
struct PathSelectiveFailBackend {
    fail_substring: &'static str,
}

impl SyntaxBackend for PathSelectiveFailBackend {
    fn language(&self) -> &str {
        "rust"
    }

    fn capability(&self) -> &SyntaxCapability {
        static CAP: SyntaxCapability = SyntaxCapability {
            supported_kinds: vec![],
            supports_containers: false,
            supports_docs: false,
        };
        &CAP
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file
            .relative_path
            .to_string_lossy()
            .contains(self.fail_substring)
        {
            return Err(SyntaxError::Parse {
                path: file.relative_path.clone(),
                reason: "simulated selective failure".to_string(),
            });
        }
        Ok(SyntaxExtraction {
            language: "rust".to_string(),
            symbols: vec![SyntaxSymbol {
                name: "stub_fn".to_string(),
                qualified_name: "stub_fn".to_string(),
                kind: SymbolKind::Function,
                span: core_model::SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    start_byte: 0,
                    byte_length: 10,
                },
                signature: "fn stub_fn()".to_string(),
                docstring: None,
                parent_qualified_name: None,
            }],
            backend_id: BackendId("path-selective-fail".to_string()),
        })
    }
}

/// Build a registry with a failing backend as the only backend for Rust.
fn make_fail_only_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        FailingSyntaxBackend::backend_id(),
        Box::new(FailingSyntaxBackend),
    );
    registry
}

/// Build a registry with a path-selective-fail backend as the only backend.
fn make_selective_fail_registry(fail_substring: &'static str) -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        BackendId("path-selective-fail".to_string()),
        Box::new(PathSelectiveFailBackend { fail_substring }),
    );
    registry
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
// End-to-end pipeline smoke test (real syntax backend)
// ---------------------------------------------------------------------------

#[test]
fn pipeline_end_to_end_smoke_test() {
    let repo_dir = setup_test_repo();
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: Some("test-correlation-001".to_string()),
        use_git_diff: false,
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
    let symbol_id =
        core_model::build_symbol_id("test-repo", "src/main.rs", "main", SymbolKind::Function)
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
// End-to-end with real tree-sitter backend
// ---------------------------------------------------------------------------

#[test]
fn pipeline_end_to_end_with_treesitter_backend() {
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "treesitter-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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

    // Verify source_backend provenance on a symbol.
    let first_sym = db
        .symbols()
        .get(&symbol_ids[0])
        .expect("get symbol")
        .expect("symbol exists");
    assert!(
        first_sym.source_backend.contains("syntax-rust"),
        "source_backend should identify syntax-rust: {}",
        first_sym.source_backend
    );
    assert_eq!(
        first_sym.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );

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

    // Write a Rust file (supported) and a Python file (no backend registered).
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write rs");
    std::fs::write(repo_dir.path().join("script.py"), "print('hi')\n").expect("write py");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "mixed-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Rust file produced symbols (files_parsed), Python is file-only.
    assert_eq!(result.metrics.files_parsed, 1);
    assert_eq!(result.metrics.files_file_only, 1);

    // Python file is NOT an error — missing backend produces a file-only record.
    let py_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        py_errors.is_empty(),
        "missing backend should not produce a file error"
    );

    // Repo persisted with both files.
    let repo = db
        .repos()
        .get("mixed-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 2);

    // Python file has a file record with zero symbols.
    let py_file = db
        .files()
        .get("mixed-repo", "script.py")
        .expect("query file")
        .expect("Python file record should exist");
    assert_eq!(py_file.language, "python");
    assert_eq!(py_file.symbol_count, 0);

    // Python file blob was persisted.
    let py_content = std::fs::read(repo_dir.path().join("script.py")).expect("read py");
    let py_hash = store::content_hash(&py_content);
    assert!(blob_store.exists(&py_hash).expect("blob exists"));
}

// ---------------------------------------------------------------------------
// Backend failure and error provenance
// ---------------------------------------------------------------------------

#[test]
fn adapter_error_carries_backend_id_provenance() {
    let repo_dir = setup_test_repo();
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_fail_only_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "fail-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Backend failed — file gets a file-only record, not counted as "parsed".
    assert_eq!(result.metrics.files_parsed, 0);
    assert_eq!(result.metrics.files_file_only, 1);

    // Error carries the backend ID for provenance.
    let rs_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("main.rs"))
        .collect();
    assert!(!rs_errors.is_empty(), "expected error for main.rs");
    assert_eq!(
        rs_errors[0].backend_id.as_deref(),
        Some("failing-backend"),
        "error should carry the failing backend's ID"
    );
    assert!(
        rs_errors[0].error.contains("simulated failure"),
        "error message should contain backend error: {}",
        rs_errors[0].error
    );

    // File record still exists with zero symbols.
    let file = db
        .files()
        .get("fail-repo", "src/main.rs")
        .expect("query file")
        .expect("file record should exist despite backend failure");
    assert_eq!(file.symbol_count, 0);

    // Blob was persisted.
    let content = std::fs::read(repo_dir.path().join("src/main.rs")).expect("read file");
    let hash = store::content_hash(&content);
    assert!(blob_store.exists(&hash).expect("blob exists"));
}

// ---------------------------------------------------------------------------
// Discovery stage tests
// ---------------------------------------------------------------------------

#[test]
fn discovery_stage_finds_files() {
    let repo_dir = setup_test_repo();
    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let output = stage::discover(&ctx).expect("discovery should succeed");
    assert!(!output.files.is_empty());
    assert!(output.files.iter().any(|f| f.language == "rust"));
}

#[test]
fn discovery_stage_rejects_invalid_root() {
    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: PathBuf::from("/nonexistent/path"),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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

    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
fn parse_stage_handles_no_backend() {
    let repo_dir = setup_test_repo();
    // Write a Python file that the registry won't have a backend for.
    std::fs::write(repo_dir.path().join("script.py"), "print('hi')\n").expect("write py file");

    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let discovery = stage::discover(&ctx).expect("discovery ok");
    let parse_output = stage::parse(&ctx, &discovery);

    // The Python file should NOT appear in file_errors — missing backend
    // is not an error, it produces a file-only record.
    let py_errors: Vec<_> = parse_output
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        py_errors.is_empty(),
        "missing backend should not produce a file error"
    );

    // The Python file should appear in parsed_files as file-only.
    let py_parsed: Vec<_> = parse_output
        .parsed_files
        .iter()
        .filter(|f| f.relative_path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        !py_parsed.is_empty(),
        "Python file should be in parsed_files as file-only"
    );
    assert_eq!(py_parsed[0].merge_result.symbols.len(), 0);
}

#[test]
fn parse_stage_uses_dispatch_context_syntax_policy() {
    let repo_dir = setup_test_repo();

    // Use a dispatch context with syntax disabled. Even though a Rust backend
    // is registered, the dispatch planner should skip syntax extraction.
    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext {
            syntax_policy: SyntaxPolicy::Disabled,
            ..DispatchContext::default()
        },
        correlation_id: None,
        use_git_diff: false,
    };

    let discovery = stage::discover(&ctx).expect("discovery ok");
    let parse_output = stage::parse(&ctx, &discovery);

    // With syntax disabled, the Rust file should be file-only (no symbols).
    let rs_parsed: Vec<_> = parse_output
        .parsed_files
        .iter()
        .filter(|f| f.relative_path.to_string_lossy().contains("main.rs"))
        .collect();
    assert!(
        !rs_parsed.is_empty(),
        "Rust file should still be in parsed_files"
    );
    assert_eq!(
        rs_parsed[0].merge_result.symbols.len(),
        0,
        "syntax disabled should produce no symbols"
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "aggregate-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "symbol-cleanup-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "enrich-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();

    // Run pipeline twice with separate DBs.
    let mut db1 = store::MetadataStore::open_in_memory().expect("open store");
    let mut db2 = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "det-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
// Backend failure isolation
// ---------------------------------------------------------------------------

#[test]
fn reindex_with_backend_failure_preserves_previously_indexed_file() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    std::fs::write(src.join("lib.rs"), "pub fn helper() {}\n").expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    // First run: use the real Rust backend — both files index successfully.
    let ts_registry = make_registry();
    let ctx1 = PipelineContext {
        repo_id: "fail-isolation-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &ts_registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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

    // Second run: use a registry with a backend that fails on lib.rs.
    let fail_registry = make_selective_fail_registry("lib.rs");
    let ctx2 = PipelineContext {
        repo_id: "fail-isolation-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &fail_registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let r2 = run(&ctx2, &mut db, &blob_store).expect("second run");
    // Both files are unchanged from the first run, so incremental indexing
    // skips them entirely — the failing backend is never invoked.
    assert_eq!(r2.metrics.files_parsed, 0);
    assert_eq!(r2.metrics.files_unchanged, 2);

    // lib.rs file record should still exist (not purged by stale cleanup).
    assert!(
        db.files()
            .get("fail-isolation-repo", "src/lib.rs")
            .unwrap()
            .is_some(),
        "lib.rs metadata should be preserved despite backend failure"
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-mod-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "incr-new-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "hash-persist-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "del-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "lifecycle-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "empty-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "multi-del-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
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

// ---------------------------------------------------------------------------
// Git-diff accelerated mode integration tests
// ---------------------------------------------------------------------------

fn init_git_repo(dir: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git init");

    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("git config email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("git config name");
}

fn git_add_commit(dir: &std::path::Path, message: &str) {
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("git add");

    std::process::Command::new("git")
        .args(["commit", "-m", message, "--allow-empty"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit");
}

/// Git-diff mode produces the same final DB state as hash-based mode after
/// initial index + modify + delete lifecycle.
#[test]
fn git_diff_parity_with_hash_based_detection() {
    let registry = make_registry();

    // --- Run A: hash-based mode ---
    let repo_a = TempDir::new().expect("repo_a dir");
    let blob_a = TempDir::new().expect("blob_a dir");
    let blob_store_a = setup_blob_store(&blob_a);
    let mut db_a = store::MetadataStore::open_in_memory().expect("db_a");

    // Initial index.
    std::fs::write(repo_a.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo_a.path().join("b.rs"), "pub fn b() {}\n").unwrap();

    let ctx_a = PipelineContext {
        repo_id: "parity-repo".to_string(),
        source_root: repo_a.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };
    run(&ctx_a, &mut db_a, &blob_store_a).expect("hash run 1");

    // Modify a.rs, delete b.rs, add c.rs.
    std::fs::write(repo_a.path().join("a.rs"), "pub fn a_v2() {}\n").unwrap();
    std::fs::remove_file(repo_a.path().join("b.rs")).unwrap();
    std::fs::write(repo_a.path().join("c.rs"), "pub fn c() {}\n").unwrap();
    let r_a = run(&ctx_a, &mut db_a, &blob_store_a).expect("hash run 2");

    // --- Run B: git-diff mode ---
    let repo_b = TempDir::new().expect("repo_b dir");
    let blob_b = TempDir::new().expect("blob_b dir");
    let blob_store_b = setup_blob_store(&blob_b);
    let mut db_b = store::MetadataStore::open_in_memory().expect("db_b");

    init_git_repo(repo_b.path());
    std::fs::write(repo_b.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo_b.path().join("b.rs"), "pub fn b() {}\n").unwrap();
    git_add_commit(repo_b.path(), "initial");

    let ctx_b = PipelineContext {
        repo_id: "parity-repo".to_string(),
        source_root: repo_b.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: true,
    };
    run(&ctx_b, &mut db_b, &blob_store_b).expect("git run 1");

    // Modify a.rs, delete b.rs, add c.rs — commit so git-diff can see changes.
    std::fs::write(repo_b.path().join("a.rs"), "pub fn a_v2() {}\n").unwrap();
    std::fs::remove_file(repo_b.path().join("b.rs")).unwrap();
    std::fs::write(repo_b.path().join("c.rs"), "pub fn c() {}\n").unwrap();
    git_add_commit(repo_b.path(), "modify and delete");
    let r_b = run(&ctx_b, &mut db_b, &blob_store_b).expect("git run 2");

    // Compare metrics.
    assert_eq!(
        r_a.metrics.files_parsed, r_b.metrics.files_parsed,
        "files_parsed mismatch"
    );
    assert_eq!(
        r_a.metrics.files_deleted, r_b.metrics.files_deleted,
        "files_deleted mismatch"
    );
    assert_eq!(
        r_a.metrics.symbols_extracted, r_b.metrics.symbols_extracted,
        "symbols_extracted mismatch"
    );

    // Compare final DB state: same files and symbols.
    let files_a = db_a.files().list_hash_map("parity-repo").unwrap();
    let files_b = db_b.files().list_hash_map("parity-repo").unwrap();
    assert_eq!(files_a, files_b, "file hash maps should be identical");

    // Compare symbols per file.
    for path in files_a.keys() {
        let syms_a = db_a
            .symbols()
            .list_ids_for_file("parity-repo", path)
            .unwrap();
        let syms_b = db_b
            .symbols()
            .list_ids_for_file("parity-repo", path)
            .unwrap();
        assert_eq!(syms_a, syms_b, "symbol IDs for {path} should be identical");
    }
}

/// Git-diff mode detects uncommitted working-tree changes (same HEAD).
#[test]
fn git_diff_detects_uncommitted_changes() {
    let registry = make_registry();
    let repo_dir = TempDir::new().expect("repo dir");
    let blob_dir = TempDir::new().expect("blob dir");
    let blob_store = setup_blob_store(&blob_dir);
    let mut db = store::MetadataStore::open_in_memory().expect("db");

    init_git_repo(repo_dir.path());
    std::fs::write(repo_dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    git_add_commit(repo_dir.path(), "initial");

    let ctx = PipelineContext {
        repo_id: "dirty-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: true,
    };

    // Initial index — a.rs is new.
    let r1 = run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(r1.metrics.files_parsed, 1);

    // Modify a.rs WITHOUT committing.
    std::fs::write(repo_dir.path().join("a.rs"), "pub fn a_modified() {}\n").unwrap();

    // Second run — HEAD is the same but file is dirty; must reindex.
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_parsed, 1, "dirty file must be re-parsed");
    assert_eq!(
        r2.metrics.files_unchanged, 0,
        "dirty file must not be skipped"
    );

    // Verify the updated symbol is in the DB.
    let syms = db
        .symbols()
        .list_ids_for_file("dirty-repo", "a.rs")
        .unwrap();
    assert!(
        !syms.is_empty(),
        "a.rs should have symbols after dirty reindex"
    );
}

/// Git-diff mode persists git_head and uses it for subsequent runs.
#[test]
fn git_diff_persists_and_uses_git_head() {
    let registry = make_registry();
    let repo_dir = TempDir::new().expect("repo dir");
    let blob_dir = TempDir::new().expect("blob dir");
    let blob_store = setup_blob_store(&blob_dir);
    let mut db = store::MetadataStore::open_in_memory().expect("db");

    init_git_repo(repo_dir.path());
    std::fs::write(repo_dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    git_add_commit(repo_dir.path(), "initial");

    let ctx = PipelineContext {
        repo_id: "head-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: true,
    };

    run(&ctx, &mut db, &blob_store).expect("run 1");

    // Verify git_head was persisted.
    let repo_record = db.repos().get("head-repo").unwrap().unwrap();
    assert!(
        repo_record.git_head.is_some(),
        "git_head should be persisted"
    );
    let head1 = repo_record.git_head.unwrap();
    assert_eq!(head1.len(), 40, "should be full SHA");

    // Make a new commit and reindex.
    std::fs::write(repo_dir.path().join("b.rs"), "pub fn b() {}\n").unwrap();
    git_add_commit(repo_dir.path(), "add b");

    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(
        r2.metrics.files_parsed, 1,
        "only new file b.rs should be parsed"
    );
    assert_eq!(r2.metrics.files_unchanged, 1, "a.rs should be unchanged");

    // Verify git_head was updated.
    let repo_record2 = db.repos().get("head-repo").unwrap().unwrap();
    let head2 = repo_record2.git_head.unwrap();
    assert_ne!(head1, head2, "git_head should have been updated");
}

/// Git-diff mode with no previous head falls back to hash-based (first run).
#[test]
fn git_diff_first_run_indexes_all_files() {
    let registry = make_registry();
    let repo_dir = TempDir::new().expect("repo dir");
    let blob_dir = TempDir::new().expect("blob dir");
    let blob_store = setup_blob_store(&blob_dir);
    let mut db = store::MetadataStore::open_in_memory().expect("db");

    init_git_repo(repo_dir.path());
    std::fs::write(repo_dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(repo_dir.path().join("b.rs"), "pub fn b() {}\n").unwrap();
    git_add_commit(repo_dir.path(), "initial");

    let ctx = PipelineContext {
        repo_id: "first-run-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: true,
    };

    let r = run(&ctx, &mut db, &blob_store).expect("run 1");
    assert_eq!(
        r.metrics.files_parsed, 2,
        "all files should be parsed on first run"
    );
    assert_eq!(r.metrics.files_unchanged, 0);
}

// ---------------------------------------------------------------------------
// File-only indexing: recognized files without syntax backends (#166)
// ---------------------------------------------------------------------------

/// A recognized-language repo with no backends at all should produce file
/// records and blobs for all discovered files (non-empty index).
#[test]
fn file_only_indexing_recognized_repo_no_backends() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("app.py"), "print('hello')\n").expect("write py");
    std::fs::write(repo_dir.path().join("lib.go"), "package main\n").expect("write go");
    std::fs::write(repo_dir.path().join("index.js"), "console.log('hi');\n").expect("write js");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    // Registry only has Rust backend — no backends for Python/Go/JS.
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "no-adapter-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // All 3 files indexed as file-only — no symbol-bearing backend output.
    assert_eq!(result.metrics.files_parsed, 0);
    assert_eq!(result.metrics.files_file_only, 3);
    assert_eq!(result.metrics.symbols_extracted, 0);
    assert!(
        result.file_errors.is_empty(),
        "no errors expected for missing backends"
    );

    // Repo has all 3 file records.
    let repo = db
        .repos()
        .get("no-adapter-repo")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 3);
    assert_eq!(repo.symbol_count, 0);

    // Each file has a file record with zero symbols.
    for (path, lang) in &[
        ("app.py", "python"),
        ("lib.go", "go"),
        ("index.js", "javascript"),
    ] {
        let file = db
            .files()
            .get("no-adapter-repo", path)
            .expect("query file")
            .unwrap_or_else(|| panic!("file record for {} should exist", path));
        assert_eq!(file.language, *lang);
        assert_eq!(file.symbol_count, 0);
    }

    // Blobs were persisted for all files.
    for name in &["app.py", "lib.go", "index.js"] {
        let content = std::fs::read(repo_dir.path().join(name)).expect("read file");
        let hash = store::content_hash(&content);
        assert!(
            blob_store.exists(&hash).expect("blob exists"),
            "blob for {} should be persisted",
            name
        );
    }
}

/// A mixed repo with both symbol-bearing and file-only files should persist
/// both correctly.
#[test]
fn file_only_indexing_mixed_repo() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Rust file — will get symbols from syntax backend.
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write rs");
    // Python file — no backend, will be file-only.
    std::fs::write(repo_dir.path().join("script.py"), "import sys\n").expect("write py");
    // SQL file — recognized language, no backend.
    std::fs::write(
        repo_dir.path().join("schema.sql"),
        "CREATE TABLE t (id INT);\n",
    )
    .expect("write sql");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "mixed-file-only".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // 1 with symbols (Rust), 2 file-only (Python, SQL).
    assert_eq!(result.metrics.files_parsed, 1);
    assert_eq!(result.metrics.files_file_only, 2);
    assert!(
        result.metrics.symbols_extracted >= 1,
        "Rust should have symbols"
    );

    // Repo aggregates.
    let repo = db
        .repos()
        .get("mixed-file-only")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 3);
    assert!(repo.symbol_count >= 1);

    // Rust file has symbols.
    let rs_file = db
        .files()
        .get("mixed-file-only", "src/main.rs")
        .expect("query file")
        .expect("Rust file record");
    assert!(rs_file.symbol_count >= 1);

    // Python file is file-only.
    let py_file = db
        .files()
        .get("mixed-file-only", "script.py")
        .expect("query file")
        .expect("Python file record");
    assert_eq!(py_file.symbol_count, 0);
}

/// Stale file cleanup should not delete file-only indexed entries on
/// re-index when the file is still present on disk.
#[test]
fn file_only_stale_cleanup_preserves_file_only_records() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("app.py"), "print('v1')\n").expect("write py");
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "stale-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    // First index: 1 symbol-bearing (Rust), 1 file-only (Python).
    let r1 = run(&ctx, &mut db, &blob_store).expect("first run");
    assert_eq!(r1.metrics.files_parsed, 1);
    assert_eq!(r1.metrics.files_file_only, 1);

    // Re-index without changes: file-only record should survive.
    let _r2 = run(&ctx, &mut db, &blob_store).expect("second run");

    let repo = db
        .repos()
        .get("stale-test")
        .expect("query repo")
        .expect("repo record");
    assert_eq!(repo.file_count, 2, "both files should still be in index");

    let py_file = db
        .files()
        .get("stale-test", "app.py")
        .expect("query file")
        .expect("Python file record should survive re-index");
    assert_eq!(py_file.symbol_count, 0);
}

/// Verify that file-only records use the same content_hash contract as
/// symbol-bearing files (store::content_hash SHA-256).
#[test]
fn file_only_uses_same_content_hash_contract() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("app.py"), "x = 1\n").expect("write py");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "hash-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    let content = std::fs::read(repo_dir.path().join("app.py")).expect("read file");
    let expected_hash = store::content_hash(&content);

    let file = db
        .files()
        .get("hash-test", "app.py")
        .expect("query file")
        .expect("file record");
    assert_eq!(
        file.file_hash, expected_hash,
        "file-only record should use the same content_hash as blob store"
    );

    // Blob retrievable by that hash.
    let blob = blob_store
        .get(&expected_hash)
        .expect("get blob")
        .expect("blob should exist");
    assert_eq!(blob, content);
}

/// Missing-backend and backend-failure produce different diagnostic states.
#[test]
fn file_only_distinguishes_missing_backend_from_backend_failure() {
    let repo_dir = TempDir::new().expect("create temp dir");
    // Python — no backend at all (missing backend).
    std::fs::write(repo_dir.path().join("script.py"), "print('hi')\n").expect("write py");
    // Rust — backend exists but fails (backend failure).
    std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").expect("write rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_fail_only_registry(); // Only returns FailingSyntaxBackend for Rust.
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "diag-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Both files get file-only records — neither produced symbols.
    assert_eq!(result.metrics.files_parsed, 0);
    assert_eq!(result.metrics.files_file_only, 2);

    // Missing backend (Python): no file_error.
    let py_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("script.py"))
        .collect();
    assert!(
        py_errors.is_empty(),
        "missing backend should not produce a file error"
    );

    // Backend failure (Rust): file_error WITH backend_id.
    let rs_errors: Vec<_> = result
        .file_errors
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("main.rs"))
        .collect();
    assert!(
        !rs_errors.is_empty(),
        "backend failure should still produce a file error"
    );
    assert_eq!(rs_errors[0].backend_id.as_deref(), Some("failing-backend"));
}

// ---------------------------------------------------------------------------
// PHP syntax backend integration tests
// ---------------------------------------------------------------------------

/// Registry with both Rust and PHP syntax backends (production-like setup).
fn make_registry_with_php() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry.register_syntax(
        PhpSyntaxBackend::backend_id(),
        Box::new(PhpSyntaxBackend::new()),
    );
    registry
}

#[test]
fn pipeline_php_laravel_controller_end_to_end() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let app = repo_dir.path().join("app/Http/Controllers");
    std::fs::create_dir_all(&app).expect("create controller dir");

    std::fs::write(
        app.join("UserController.php"),
        r#"<?php

namespace App\Http\Controllers;

use App\Models\User;
use Illuminate\Http\Request;

/**
 * Handles user-related HTTP requests.
 */
class UserController extends Controller
{
    /**
     * Display a listing of users.
     */
    public function index(): JsonResponse
    {
        return response()->json(User::all());
    }

    public function show(int $id): JsonResponse
    {
        return response()->json(User::findOrFail($id));
    }

    public function store(Request $request): JsonResponse
    {
        $user = User::create($request->validated());
        return response()->json($user, 201);
    }
}
"#,
    )
    .expect("write controller");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_php();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "laravel-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // PHP file was discovered and parsed.
    assert!(
        result.metrics.files_parsed >= 1,
        "expected PHP file to be parsed, files_parsed={}",
        result.metrics.files_parsed
    );
    assert!(result.file_errors.is_empty(), "no file errors expected");

    // Verify file record exists with symbols.
    let file = db
        .files()
        .get("laravel-repo", "app/Http/Controllers/UserController.php")
        .expect("query file")
        .expect("UserController.php file record should exist");
    assert_eq!(file.language, "php");
    assert!(
        file.symbol_count >= 4,
        "expected at least UserController + 3 methods, got {}",
        file.symbol_count
    );
    assert_eq!(file.capability_tier, core_model::CapabilityTier::SyntaxOnly);

    // Verify symbol records exist with correct provenance.
    let symbol_ids = db
        .symbols()
        .list_ids_for_file("laravel-repo", "app/Http/Controllers/UserController.php")
        .expect("list symbols");
    assert!(
        symbol_ids.len() >= 4,
        "expected at least 4 symbols, got {}",
        symbol_ids.len()
    );

    // Find the UserController symbol and verify it.
    let all_syms: Vec<_> = symbol_ids
        .iter()
        .filter_map(|id| db.symbols().get(id).ok().flatten())
        .collect();

    let controller = all_syms
        .iter()
        .find(|s| s.name == "UserController")
        .expect("UserController symbol should exist");
    assert_eq!(controller.kind, SymbolKind::Class);
    assert!(controller.source_backend.contains("syntax-php"));
    assert_eq!(
        controller.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );

    // Verify a method has correct qualified name with namespace.
    let index_method = all_syms
        .iter()
        .find(|s| s.name == "index")
        .expect("index method should exist");
    assert_eq!(index_method.kind, SymbolKind::Method);
    assert!(
        index_method
            .qualified_name
            .contains("UserController::index"),
        "qualified_name should contain UserController::index, got: {}",
        index_method.qualified_name
    );
}

#[test]
fn pipeline_php_laravel_model_end_to_end() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let models = repo_dir.path().join("app/Models");
    std::fs::create_dir_all(&models).expect("create models dir");

    std::fs::write(
        models.join("Post.php"),
        r#"<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

/**
 * Eloquent model for the posts table.
 */
class Post extends Model
{
    const STATUS_DRAFT = 'draft';
    const STATUS_PUBLISHED = 'published';

    public function comments(): HasMany
    {
        return $this->hasMany(Comment::class);
    }

    public function publish(): void
    {
        $this->status = self::STATUS_PUBLISHED;
        $this->save();
    }
}
"#,
    )
    .expect("write model");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_php();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "laravel-model-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
    assert!(result.file_errors.is_empty());

    let file = db
        .files()
        .get("laravel-model-repo", "app/Models/Post.php")
        .expect("query file")
        .expect("Post.php file record");
    assert_eq!(file.language, "php");
    // Post class + 2 constants + 2 methods = 5 minimum
    assert!(
        file.symbol_count >= 5,
        "expected at least 5 symbols, got {}",
        file.symbol_count
    );

    let symbol_ids = db
        .symbols()
        .list_ids_for_file("laravel-model-repo", "app/Models/Post.php")
        .expect("list symbols");
    let all_syms: Vec<_> = symbol_ids
        .iter()
        .filter_map(|id| db.symbols().get(id).ok().flatten())
        .collect();

    // Verify class constants exist with qualified names.
    let draft = all_syms
        .iter()
        .find(|s| s.name == "STATUS_DRAFT")
        .expect("STATUS_DRAFT should exist");
    assert_eq!(draft.kind, SymbolKind::Constant);
    assert!(
        draft.qualified_name.contains("Post::STATUS_DRAFT"),
        "expected namespaced qualified name, got: {}",
        draft.qualified_name
    );

    // Verify methods.
    let comments = all_syms
        .iter()
        .find(|s| s.name == "comments")
        .expect("comments method should exist");
    assert_eq!(comments.kind, SymbolKind::Method);
}

#[test]
fn pipeline_php_mixed_language_repo() {
    let repo_dir = TempDir::new().expect("create temp dir");

    // PHP file
    std::fs::write(
        repo_dir.path().join("index.php"),
        "<?php\nfunction main(): void {}\n",
    )
    .expect("write php");

    // Rust file
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::write(src.join("lib.rs"), "pub fn greet() {}\n").expect("write rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_php();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "mixed-lang-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    // Both PHP and Rust files should be parsed with symbols.
    assert!(
        result.metrics.files_parsed >= 2,
        "expected both PHP and Rust parsed, got {}",
        result.metrics.files_parsed
    );

    // Verify PHP file.
    let php_file = db
        .files()
        .get("mixed-lang-repo", "index.php")
        .expect("query php")
        .expect("index.php record");
    assert_eq!(php_file.language, "php");
    assert!(php_file.symbol_count >= 1);
    assert_eq!(
        php_file.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );

    // Verify Rust file.
    let rs_file = db
        .files()
        .get("mixed-lang-repo", "src/lib.rs")
        .expect("query rs")
        .expect("lib.rs record");
    assert_eq!(rs_file.language, "rust");
    assert!(rs_file.symbol_count >= 1);
    assert_eq!(
        rs_file.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );
}

// ---------------------------------------------------------------------------
// Python syntax backend integration tests
// ---------------------------------------------------------------------------

fn make_registry_with_python() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry.register_syntax(
        PythonSyntaxBackend::backend_id(),
        Box::new(PythonSyntaxBackend::new()),
    );
    registry
}

#[test]
fn pipeline_python_django_model_end_to_end() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let models = repo_dir.path().join("app");
    std::fs::create_dir_all(&models).expect("create app dir");

    std::fs::write(
        models.join("models.py"),
        r#"
class Article:
    """Represents an article."""

    def __init__(self, title: str, body: str) -> None:
        """Initialize an article."""
        self.title = title
        self.body = body

    def publish(self) -> None:
        """Mark the article as published."""
        self.published = True

    @classmethod
    def from_dict(cls, data: dict) -> "Article":
        return cls(data["title"], data["body"])

def create_article(title: str, body: str) -> "Article":
    """Factory function for articles."""
    return Article(title, body)
"#,
    )
    .expect("write models.py");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_python();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "python-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    assert!(
        result.metrics.files_parsed >= 1,
        "expected Python file to be parsed, files_parsed={}",
        result.metrics.files_parsed
    );
    assert!(result.file_errors.is_empty(), "no file errors expected");

    let file = db
        .files()
        .get("python-repo", "app/models.py")
        .expect("query file")
        .expect("models.py file record");
    assert_eq!(file.language, "python");
    // Article class + __init__ + publish + from_dict + create_article = 5 minimum
    assert!(
        file.symbol_count >= 5,
        "expected at least 5 symbols, got {}",
        file.symbol_count
    );
    assert_eq!(file.capability_tier, core_model::CapabilityTier::SyntaxOnly);

    let symbol_ids = db
        .symbols()
        .list_ids_for_file("python-repo", "app/models.py")
        .expect("list symbols");
    let all_syms: Vec<_> = symbol_ids
        .iter()
        .filter_map(|id| db.symbols().get(id).ok().flatten())
        .collect();

    let article = all_syms
        .iter()
        .find(|s| s.name == "Article")
        .expect("Article should exist");
    assert_eq!(article.kind, SymbolKind::Class);
    assert!(article.source_backend.contains("syntax-python"));

    let publish = all_syms
        .iter()
        .find(|s| s.name == "publish")
        .expect("publish should exist");
    assert_eq!(publish.kind, SymbolKind::Method);
    assert!(
        publish.qualified_name.contains("Article::publish"),
        "expected qualified name, got: {}",
        publish.qualified_name
    );

    let factory = all_syms
        .iter()
        .find(|s| s.name == "create_article")
        .expect("create_article should exist");
    assert_eq!(factory.kind, SymbolKind::Function);
}

#[test]
fn pipeline_python_mixed_with_rust() {
    let repo_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        repo_dir.path().join("main.py"),
        "class App:\n    def run(self) -> None:\n        pass\n",
    )
    .expect("write py");

    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::write(src.join("lib.rs"), "pub fn greet() {}\n").expect("write rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_python();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "py-rs-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    assert!(
        result.metrics.files_parsed >= 2,
        "expected both Python and Rust parsed"
    );

    let py_file = db
        .files()
        .get("py-rs-repo", "main.py")
        .expect("query py")
        .expect("main.py record");
    assert_eq!(py_file.language, "python");
    assert!(py_file.symbol_count >= 2); // App + run
    assert_eq!(
        py_file.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );
}

// ---------------------------------------------------------------------------
// Go syntax backend integration tests
// ---------------------------------------------------------------------------

fn make_registry_with_go() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    registry.register_syntax(
        RustSyntaxBackend::backend_id(),
        Box::new(RustSyntaxBackend::new()),
    );
    registry.register_syntax(
        GoSyntaxBackend::backend_id(),
        Box::new(GoSyntaxBackend::new()),
    );
    registry
}

#[test]
fn pipeline_go_http_server_end_to_end() {
    let repo_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        repo_dir.path().join("main.go"),
        r#"package main

// Server holds HTTP server configuration.
type Server struct {
    Addr string
    Port int
}

// NewServer creates a new server.
func NewServer(addr string, port int) *Server {
    return &Server{Addr: addr, Port: port}
}

// Start starts the server.
func (s *Server) Start() error {
    return nil
}

const DefaultPort = 8080
"#,
    )
    .expect("write main.go");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_go();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "go-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");

    assert!(
        result.metrics.files_parsed >= 1,
        "expected Go file to be parsed"
    );
    assert!(result.file_errors.is_empty());

    let file = db
        .files()
        .get("go-repo", "main.go")
        .expect("query file")
        .expect("main.go file record");
    assert_eq!(file.language, "go");
    // Server type + NewServer func + Start method + DefaultPort const = 4
    assert!(
        file.symbol_count >= 4,
        "expected at least 4 symbols, got {}",
        file.symbol_count
    );
    assert_eq!(file.capability_tier, core_model::CapabilityTier::SyntaxOnly);

    let symbol_ids = db
        .symbols()
        .list_ids_for_file("go-repo", "main.go")
        .expect("list symbols");
    let all_syms: Vec<_> = symbol_ids
        .iter()
        .filter_map(|id| db.symbols().get(id).ok().flatten())
        .collect();

    let server = all_syms
        .iter()
        .find(|s| s.name == "Server")
        .expect("Server should exist");
    assert_eq!(server.kind, SymbolKind::Type);
    assert!(server.source_backend.contains("syntax-go"));

    let start = all_syms
        .iter()
        .find(|s| s.name == "Start")
        .expect("Start should exist");
    assert_eq!(start.kind, SymbolKind::Method);
    assert!(
        start.qualified_name.contains("Server::Start"),
        "expected receiver-qualified name, got: {}",
        start.qualified_name
    );
}

#[test]
fn pipeline_go_mixed_with_rust() {
    let repo_dir = TempDir::new().expect("create temp dir");

    std::fs::write(
        repo_dir.path().join("main.go"),
        "package main\nfunc Run() error {\n    return nil\n}\n",
    )
    .expect("write go");

    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::write(src.join("lib.rs"), "pub fn greet() {}\n").expect("write rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let registry = make_registry_with_go();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "go-rs-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id: None,
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed");
    assert!(result.metrics.files_parsed >= 2);

    let go_file = db
        .files()
        .get("go-rs-repo", "main.go")
        .expect("query go")
        .expect("main.go record");
    assert_eq!(go_file.language, "go");
    assert!(go_file.symbol_count >= 1);
    assert_eq!(
        go_file.capability_tier,
        core_model::CapabilityTier::SyntaxOnly
    );
}

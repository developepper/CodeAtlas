//! Integration tests for the indexer pipeline.
//!
//! These tests exercise the full discovery → parse → persist flow against
//! real temp-dir repositories with an in-memory SQLite store.

use std::path::PathBuf;

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, AdapterPolicy, AdapterRouter,
    ExtractedSymbol, IndexContext, LanguageAdapter, SourceFile, SourceSpan,
};
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

// ---------------------------------------------------------------------------
// End-to-end pipeline smoke test
// ---------------------------------------------------------------------------

#[test]
fn pipeline_end_to_end_smoke_test() {
    let repo_dir = setup_test_repo();
    let router = StubRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "test-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("test-correlation-001".to_string()),
    };

    let result = run(&ctx, &mut db).expect("pipeline should succeed");

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

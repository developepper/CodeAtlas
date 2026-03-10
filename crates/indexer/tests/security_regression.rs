//! Security regression tests for the indexer pipeline.
//!
//! Covers:
//! - Malformed source files (garbage, empty, null bytes, extreme lines).
//! - Pipeline resilience: parse errors are captured, not panics.
//! - Resource exhaustion scenarios handled by the pipeline.
//!
//! Spec references: §11.2 Controls, §16.1 Security tests.

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, AdapterPolicy, AdapterRouter,
    ExtractedSymbol, IndexContext, LanguageAdapter, SourceFile, SourceSpan,
};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use core_model::{QualityLevel, SymbolKind};
use indexer::{run, PipelineContext};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Stub adapter that always succeeds (for files where we test discovery/persist)
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
                name: "stub_fn".to_string(),
                qualified_name: "stub_fn".to_string(),
                kind: SymbolKind::Function,
                span: SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    start_byte: 0,
                    byte_length: file.content.len() as u64,
                },
                signature: "fn stub_fn()".to_string(),
                confidence_score: None,
                docstring: None,
                parent_qualified_name: None,
            }],
            source_adapter: "stub-syntax-rust".to_string(),
            quality_level: QualityLevel::Syntax,
        })
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

/// Router backed by real tree-sitter adapters for realistic parse testing.
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

fn setup_blob_store(dir: &TempDir) -> store::BlobStore {
    store::BlobStore::open(&dir.path().join("blobs")).expect("open blob store")
}

// ---------------------------------------------------------------------------
// Malformed source file tests (tree-sitter adapter)
// ---------------------------------------------------------------------------

/// Completely garbage content in a .rs file should result in a parse error
/// or zero symbols, not a panic or crash.
#[test]
fn garbage_rust_source_does_not_panic() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        src.join("garbage.rs"),
        "aslkdjf 098234 !@#$%^& garbage that is definitely not rust\n",
    )
    .expect("write garbage");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-garbage".to_string()),
        use_git_diff: false,
    };

    // Pipeline must complete without panic.
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should not panic on garbage");

    // The garbage file was discovered but may have zero symbols or file errors.
    assert!(
        result.metrics.files_discovered >= 1,
        "garbage file should be discovered"
    );
}

/// An empty .rs file should be processed without panics. Tree-sitter
/// handles empty input gracefully, yielding zero symbols.
#[test]
fn empty_rust_file_does_not_panic() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("empty.rs"), "").expect("write empty");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-empty".to_string()),
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should handle empty file");
    assert!(
        result.metrics.files_discovered >= 1,
        "empty file should be discovered"
    );
}

/// A file containing a single extremely long line should not cause OOM
/// or excessive processing time in the tree-sitter adapter.
#[test]
fn extremely_long_line_does_not_crash() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // 500KB single line of repeated tokens — valid-ish Rust syntax.
    let long_line = format!("fn f() {{ let x = \"{}\"; }}\n", "a".repeat(500_000));
    std::fs::write(src.join("long_line.rs"), long_line).expect("write long line");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-longline".to_string()),
        use_git_diff: false,
    };

    // Must complete without panic or timeout.
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should handle long lines");
    assert!(result.metrics.files_discovered >= 1);
}

/// Deeply nested syntax (many nested blocks) should not stack-overflow
/// the tree-sitter parser.
#[test]
fn deeply_nested_syntax_does_not_stack_overflow() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Generate fn main() { { { { ... } } } } with 200 nesting levels.
    let depth = 200;
    let opens: String = "{ ".repeat(depth);
    let closes: String = "} ".repeat(depth);
    let code = format!("fn main() {opens}{closes}\n");
    std::fs::write(src.join("nested.rs"), code).expect("write nested");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-nested".to_string()),
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should handle deep nesting");
    assert!(result.metrics.files_discovered >= 1);
}

// ---------------------------------------------------------------------------
// Pipeline resilience tests
// ---------------------------------------------------------------------------

/// Multiple files where some are valid and some are garbage: the pipeline
/// must succeed, reporting errors for bad files without aborting.
#[test]
fn pipeline_continues_past_parse_errors() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Valid file.
    std::fs::write(
        src.join("good.rs"),
        "pub fn hello() -> &'static str { \"world\" }\n",
    )
    .expect("write good");

    // Garbage file — still Rust by extension, but unparseable.
    std::fs::write(src.join("bad.rs"), "!!!! not valid rust at all @@@@\n").expect("write bad");

    // Another valid file.
    std::fs::write(src.join("also_good.rs"), "fn main() {}\n").expect("write also_good");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-mixed".to_string()),
        use_git_diff: false,
    };

    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should succeed with mixed files");

    // All three files should be discovered.
    assert!(
        result.metrics.files_discovered >= 3,
        "all files should be discovered: got {}",
        result.metrics.files_discovered
    );

    // At least the valid files should produce symbols.
    assert!(
        result.metrics.symbols_extracted >= 1,
        "valid files should produce symbols: got {}",
        result.metrics.symbols_extracted
    );
}

/// A repository with no parseable files (all unknown languages) should
/// produce a clean result with zero parsed files.
#[test]
fn repo_with_no_parseable_files_succeeds() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(repo_dir.path().join("data.csv"), "a,b,c\n1,2,3\n").expect("write csv");
    std::fs::write(repo_dir.path().join("notes.txt"), "just some text\n").expect("write txt");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = StubRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-noparseable".to_string()),
        use_git_diff: false,
    };

    let result =
        run(&ctx, &mut db, &blob_store).expect("pipeline should succeed with no parseable files");
    assert_eq!(
        result.metrics.symbols_extracted, 0,
        "no symbols expected from unparseable files"
    );
}

/// Invalid UTF-8 content in a source file should not crash the pipeline.
/// The walker may skip it as binary (null-byte heuristic) or the adapter
/// may handle the lossy conversion gracefully.
#[test]
fn invalid_utf8_content_does_not_crash() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Valid file alongside.
    std::fs::write(src.join("good.rs"), "fn main() {}\n").expect("write good");

    // File with invalid UTF-8 sequences embedded in otherwise valid-looking Rust.
    let mut content = b"fn broken() { let s = \"".to_vec();
    content.extend_from_slice(&[0xFF, 0xFE, 0x80, 0x81]); // invalid UTF-8
    content.extend_from_slice(b"\"; }\n");
    std::fs::write(src.join("broken.rs"), &content).expect("write broken");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-utf8".to_string()),
        use_git_diff: false,
    };

    // Pipeline must not panic. The broken file may be skipped as binary
    // or parsed with lossy conversion — either is acceptable.
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should handle invalid UTF-8");
    assert!(
        result.metrics.files_discovered >= 1,
        "at least the good file should be discovered"
    );
}

/// A file with many parse errors (many malformed items) should still
/// complete in reasonable time and not produce exponential output.
#[test]
fn many_parse_errors_complete_in_bounded_time() {
    let repo_dir = TempDir::new().expect("create temp dir");
    let src = repo_dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    // Generate a file with 1000 malformed function declarations.
    let mut content = String::new();
    for i in 0..1000 {
        content.push_str(&format!("fn broken_{i}( !!! ) {{ }}\n"));
    }
    std::fs::write(src.join("many_errors.rs"), &content).expect("write many errors");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = setup_blob_store(&blob_dir);
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().expect("open store");

    let ctx = PipelineContext {
        repo_id: "security-test".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: Some("sec-manyerr".to_string()),
        use_git_diff: false,
    };

    let start = std::time::Instant::now();
    let result = run(&ctx, &mut db, &blob_store).expect("pipeline should handle many errors");
    let elapsed = start.elapsed();

    assert!(result.metrics.files_discovered >= 1);
    assert!(
        elapsed.as_secs() < 30,
        "pipeline with many parse errors should complete quickly, took {elapsed:?}"
    );
}

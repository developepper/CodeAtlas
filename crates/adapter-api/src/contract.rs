//! Adapter contract test harness.
//!
//! Provides reusable assertion functions that verify any [`LanguageAdapter`]
//! implementation satisfies the behavioral contracts required by the platform.
//! Enable the `test-harness` feature in `dev-dependencies` to use this module.
//!
//! # Usage
//!
//! ```ignore
//! use adapter_api::contract::{ContractFixture, run_all_contracts};
//!
//! let adapter = create_my_adapter("rust").unwrap();
//! let fixture = ContractFixture::rust_baseline();
//! run_all_contracts(&adapter, &fixture);
//! ```

use std::path::PathBuf;

use core_model::Validate;

use crate::{AdapterError, IndexContext, LanguageAdapter, SourceFile};

// ---------------------------------------------------------------------------
// Fixture definition
// ---------------------------------------------------------------------------

/// A source file fixture for contract testing.
///
/// Adapters provide fixtures appropriate to the languages they support.
/// The harness functions use these to drive extraction and validate output.
pub struct ContractFixture {
    /// Language identifier (must match the adapter's `language()`).
    pub language: String,
    /// Source code bytes.
    pub source_code: Vec<u8>,
    /// Relative path for the fixture file.
    pub relative_path: PathBuf,
    /// Minimum number of symbols expected from this fixture.
    /// Set to at least 1 to verify extraction is non-trivial.
    pub expected_min_symbols: usize,
    /// Symbol names that MUST appear in the output.
    pub expected_symbol_names: Vec<String>,
}

impl ContractFixture {
    /// A baseline Rust fixture covering common symbol kinds.
    #[must_use]
    pub fn rust_baseline() -> Self {
        Self {
            language: "rust".to_string(),
            source_code: RUST_FIXTURE.as_bytes().to_vec(),
            relative_path: PathBuf::from("src/lib.rs"),
            expected_min_symbols: 5,
            expected_symbol_names: vec![
                "Config".to_string(),
                "new".to_string(),
                "process".to_string(),
                "Mode".to_string(),
                "MAX_SIZE".to_string(),
            ],
        }
    }

    /// A baseline TypeScript fixture covering common symbol kinds.
    #[must_use]
    pub fn typescript_baseline() -> Self {
        Self {
            language: "typescript".to_string(),
            source_code: TYPESCRIPT_FIXTURE.as_bytes().to_vec(),
            relative_path: PathBuf::from("src/config.ts"),
            expected_min_symbols: 5,
            expected_symbol_names: vec![
                "Config".to_string(),
                "create".to_string(),
                "process".to_string(),
                "Mode".to_string(),
                "MAX_SIZE".to_string(),
            ],
        }
    }

    /// A baseline Kotlin fixture covering common symbol kinds.
    #[must_use]
    pub fn kotlin_baseline() -> Self {
        Self {
            language: "kotlin".to_string(),
            source_code: KOTLIN_FIXTURE.as_bytes().to_vec(),
            relative_path: PathBuf::from("src/Config.kt"),
            expected_min_symbols: 5,
            expected_symbol_names: vec![
                "Config".to_string(),
                "create".to_string(),
                "process".to_string(),
                "Mode".to_string(),
                "MAX_SIZE".to_string(),
            ],
        }
    }
}

/// Baseline Rust source fixture used by contract tests.
const RUST_FIXTURE: &str = r#"/// Configuration for processing.
pub struct Config {
    pub name: String,
    pub limit: usize,
}

impl Config {
    /// Creates a new config with defaults.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            limit: 100,
        }
    }

    /// Processes the config.
    pub fn process(&self) -> bool {
        self.limit > 0
    }
}

/// Operating mode.
pub enum Mode {
    Fast,
    Precise,
}

pub const MAX_SIZE: usize = 1024;
"#;

/// Baseline TypeScript source fixture used by contract tests.
const TYPESCRIPT_FIXTURE: &str = r#"/** Configuration for processing. */
interface Config {
    name: string;
    limit: number;
}

/** Creates a new config with defaults. */
function create(name: string): Config {
    return { name, limit: 100 };
}

class Processor {
    /** Processes the config. */
    process(config: Config): boolean {
        return config.limit > 0;
    }
}

/** Operating mode. */
enum Mode {
    Fast,
    Precise,
}

const MAX_SIZE: number = 1024;
"#;

/// Baseline Kotlin source fixture used by contract tests.
const KOTLIN_FIXTURE: &str = r#"/** Configuration for processing. */
data class Config(
    val name: String,
    val limit: Int
)

/** Creates a new config with defaults. */
fun create(name: String): Config {
    return Config(name, 100)
}

class Processor {
    /** Processes the config. */
    fun process(config: Config): Boolean {
        return config.limit > 0
    }
}

/** Operating mode. */
enum class Mode {
    Fast,
    Precise
}

const val MAX_SIZE: Int = 1024
"#;

// ---------------------------------------------------------------------------
// Contract assertion functions
// ---------------------------------------------------------------------------

fn make_context() -> IndexContext {
    IndexContext {
        repo_id: "contract-test-repo".to_string(),
        source_root: PathBuf::from("/tmp/contract-test-repo"),
    }
}

fn make_source_file(fixture: &ContractFixture) -> SourceFile {
    SourceFile {
        relative_path: fixture.relative_path.clone(),
        absolute_path: PathBuf::from("/tmp/contract-test-repo").join(&fixture.relative_path),
        content: fixture.source_code.clone(),
        language: fixture.language.clone(),
    }
}

/// Asserts adapter identity fields are non-empty and stable across calls.
///
/// # Panics
/// Panics if the adapter ID or language is empty, or if repeated calls
/// return different values.
pub fn assert_adapter_identity_is_stable(adapter: &dyn LanguageAdapter) {
    let id1 = adapter.adapter_id();
    let id2 = adapter.adapter_id();
    let lang1 = adapter.language();
    let lang2 = adapter.language();

    assert!(!id1.is_empty(), "adapter_id must not be empty");
    assert!(!lang1.is_empty(), "language must not be empty");
    assert_eq!(id1, id2, "adapter_id must be stable across calls");
    assert_eq!(lang1, lang2, "language must be stable across calls");
}

/// Asserts adapter capabilities pass validation and have a valid quality level.
///
/// # Panics
/// Panics if capabilities fail [`Validate`] or have out-of-range confidence.
pub fn assert_capabilities_are_valid(adapter: &dyn LanguageAdapter) {
    let caps = adapter.capabilities();
    caps.validate()
        .expect("adapter capabilities must pass validation");
}

/// Asserts output carries correct provenance metadata.
///
/// # Panics
/// Panics if `source_adapter` doesn't match `adapter_id()` or
/// `quality_level` doesn't match capabilities.
pub fn assert_provenance_fields(adapter: &dyn LanguageAdapter, fixture: &ContractFixture) {
    let ctx = make_context();
    let file = make_source_file(fixture);
    let output = adapter
        .index_file(&ctx, &file)
        .expect("index_file must succeed for supported fixture");

    assert_eq!(
        output.source_adapter,
        adapter.adapter_id(),
        "source_adapter must match adapter_id()"
    );
    assert_eq!(
        output.quality_level,
        adapter.capabilities().quality_level,
        "output quality_level must match capabilities"
    );
}

/// Asserts extraction produces the expected symbols from the fixture.
///
/// # Panics
/// Panics if fewer symbols than `expected_min_symbols` are extracted,
/// or if any `expected_symbol_names` are missing.
pub fn assert_expected_symbols(adapter: &dyn LanguageAdapter, fixture: &ContractFixture) {
    let ctx = make_context();
    let file = make_source_file(fixture);
    let output = adapter.index_file(&ctx, &file).expect("index_file");

    assert!(
        output.symbols.len() >= fixture.expected_min_symbols,
        "expected at least {} symbols, got {}",
        fixture.expected_min_symbols,
        output.symbols.len()
    );

    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in &fixture.expected_symbol_names {
        assert!(
            names.contains(&expected.as_str()),
            "expected symbol '{}' not found in output: {:?}",
            expected,
            names
        );
    }
}

/// Asserts all extracted symbols pass validation.
///
/// # Panics
/// Panics if any symbol fails [`Validate`] or has out-of-range confidence.
pub fn assert_symbols_are_valid(adapter: &dyn LanguageAdapter, fixture: &ContractFixture) {
    let ctx = make_context();
    let file = make_source_file(fixture);
    let output = adapter.index_file(&ctx, &file).expect("index_file");

    for sym in &output.symbols {
        sym.validate()
            .unwrap_or_else(|e| panic!("symbol '{}' failed validation: {e}", sym.name));

        if let Some(score) = sym.confidence_score {
            assert!(
                (0.0..=1.0).contains(&score),
                "symbol '{}' confidence {score} out of range",
                sym.name
            );
        }

        assert!(!sym.name.trim().is_empty(), "symbol name must not be blank");
        assert!(
            !sym.qualified_name.trim().is_empty(),
            "qualified_name must not be blank"
        );
        assert!(
            sym.span.start_line >= 1,
            "symbol '{}' has zero start_line",
            sym.name
        );
        assert!(
            sym.span.byte_length > 0,
            "symbol '{}' has zero byte_length",
            sym.name
        );
    }
}

/// Asserts extraction is deterministic: two runs on the same input produce
/// identical output.
///
/// # Panics
/// Panics if symbol count, names, kinds, or spans differ between runs.
pub fn assert_extraction_is_deterministic(
    adapter: &dyn LanguageAdapter,
    fixture: &ContractFixture,
) {
    let ctx = make_context();
    let file = make_source_file(fixture);

    let out1 = adapter.index_file(&ctx, &file).expect("first run");
    let out2 = adapter.index_file(&ctx, &file).expect("second run");

    assert_eq!(
        out1.symbols.len(),
        out2.symbols.len(),
        "determinism: symbol count differs between runs"
    );
    assert_eq!(
        out1.source_adapter, out2.source_adapter,
        "determinism: source_adapter differs"
    );
    assert_eq!(
        out1.quality_level, out2.quality_level,
        "determinism: quality_level differs"
    );

    for (a, b) in out1.symbols.iter().zip(out2.symbols.iter()) {
        assert_eq!(a.name, b.name, "determinism: name differs");
        assert_eq!(a.kind, b.kind, "determinism: kind differs for '{}'", a.name);
        assert_eq!(a.span, b.span, "determinism: span differs for '{}'", a.name);
        assert_eq!(
            a.qualified_name, b.qualified_name,
            "determinism: qualified_name differs for '{}'",
            a.name
        );
        assert_eq!(
            a.confidence_score, b.confidence_score,
            "determinism: confidence_score differs for '{}'",
            a.name
        );
        assert_eq!(
            a.signature, b.signature,
            "determinism: signature differs for '{}'",
            a.name
        );
        assert_eq!(
            a.docstring, b.docstring,
            "determinism: docstring differs for '{}'",
            a.name
        );
        assert_eq!(
            a.parent_qualified_name, b.parent_qualified_name,
            "determinism: parent_qualified_name differs for '{}'",
            a.name
        );
    }
}

/// Asserts the adapter returns an error for an unsupported language.
///
/// # Panics
/// Panics if `index_file` succeeds for a language the adapter does not handle.
pub fn assert_unsupported_language_rejected(adapter: &dyn LanguageAdapter) {
    let ctx = make_context();
    // Use a language guaranteed to differ from the adapter's declared language.
    let bogus_lang = format!("not-{}", adapter.language());
    let file = SourceFile {
        relative_path: PathBuf::from("bogus.xyz"),
        absolute_path: PathBuf::from("/tmp/contract-test-repo/bogus.xyz"),
        content: b"bogus content".to_vec(),
        language: bogus_lang,
    };

    let result = adapter.index_file(&ctx, &file);
    assert!(
        result.is_err(),
        "adapter must reject unsupported language, but got Ok"
    );

    match result.unwrap_err() {
        AdapterError::Unsupported { language } => {
            assert!(
                !language.is_empty(),
                "Unsupported error must include language"
            );
        }
        other => panic!("expected AdapterError::Unsupported, got: {other}"),
    }
}

/// Asserts an empty file produces zero symbols but valid provenance.
///
/// # Panics
/// Panics if symbols are extracted from an empty file, or provenance is missing.
pub fn assert_empty_file_produces_no_symbols(adapter: &dyn LanguageAdapter) {
    let ctx = make_context();
    let file = SourceFile {
        relative_path: PathBuf::from("empty.rs"),
        absolute_path: PathBuf::from("/tmp/contract-test-repo/empty.rs"),
        content: Vec::new(),
        language: adapter.language().to_string(),
    };

    let output = adapter
        .index_file(&ctx, &file)
        .expect("empty file must not cause an error");

    assert!(
        output.symbols.is_empty(),
        "empty file must produce zero symbols, got {}",
        output.symbols.len()
    );
    assert_eq!(
        output.source_adapter,
        adapter.adapter_id(),
        "provenance must be present even for empty output"
    );
}

// ---------------------------------------------------------------------------
// Aggregate runner
// ---------------------------------------------------------------------------

/// Runs all contract assertions against the given adapter and fixture.
///
/// This is the primary entry point for adapter contract testing. Call this
/// from each adapter's integration tests to verify full contract compliance.
///
/// # Panics
/// Panics on any contract violation.
pub fn run_all_contracts(adapter: &dyn LanguageAdapter, fixture: &ContractFixture) {
    assert_adapter_identity_is_stable(adapter);
    assert_capabilities_are_valid(adapter);
    assert_provenance_fields(adapter, fixture);
    assert_expected_symbols(adapter, fixture);
    assert_symbols_are_valid(adapter, fixture);
    assert_extraction_is_deterministic(adapter, fixture);
    assert_unsupported_language_rejected(adapter);
    assert_empty_file_produces_no_symbols(adapter);
}

//! Contract test suite for the Kotlin semantic adapter.
//!
//! Uses the shared contract harness from `adapter-api` to verify that
//! the Kotlin semantic adapter satisfies all behavioral contracts.
//!
//! These tests use a mock runtime to avoid requiring a real JVM process.
//! The mock returns analysis responses that match the fixture source code,
//! verifying the mapping logic end-to-end.

use adapter_api::contract::{self, ContractFixture};
use adapter_api::LanguageAdapter;
use adapter_semantic_kotlin::adapter::KotlinSemanticAdapter;
use adapter_semantic_kotlin::error::KotlinAnalysisError;
use adapter_semantic_kotlin::protocol::KotlinResponse;
use adapter_semantic_kotlin::runtime::KotlinRuntime;

/// A mock runtime that returns a canned analysis response matching the
/// Kotlin baseline fixture from the contract test harness.
struct FixtureRuntime;

impl KotlinRuntime for FixtureRuntime {
    fn start(&mut self) -> Result<(), KotlinAnalysisError> {
        Ok(())
    }

    fn stop(&mut self) {}

    fn restart(&mut self) -> Result<(), KotlinAnalysisError> {
        Ok(())
    }

    fn is_healthy(&mut self) -> bool {
        true
    }

    fn send_request(
        &mut self,
        command: &str,
        _arguments: Option<serde_json::Value>,
    ) -> Result<KotlinResponse, KotlinAnalysisError> {
        match command {
            "analyze" => Ok(KotlinResponse {
                seq: 0,
                msg_type: "response".to_string(),
                command: Some("analyze".to_string()),
                request_seq: Some(1),
                success: Some(true),
                body: Some(fixture_analysis_body()),
                message: None,
            }),
            _ => Ok(KotlinResponse {
                seq: 0,
                msg_type: "response".to_string(),
                command: Some(command.to_string()),
                request_seq: Some(1),
                success: Some(true),
                body: None,
                message: None,
            }),
        }
    }
}

/// Returns an analysis response body matching the Kotlin baseline fixture.
///
/// The fixture source is:
/// ```kotlin
/// /** Configuration for processing. */
/// data class Config(
///     val name: String,
///     val limit: Int
/// )
///
/// /** Creates a new config with defaults. */
/// fun create(name: String): Config {
///     return Config(name, 100)
/// }
///
/// class Processor {
///     /** Processes the config. */
///     fun process(config: Config): Boolean {
///         return config.limit > 0
///     }
/// }
///
/// /** Operating mode. */
/// enum class Mode {
///     Fast,
///     Precise
/// }
///
/// const val MAX_SIZE: Int = 1024
/// ```
fn fixture_analysis_body() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "Config",
            "kind": "class",
            "modifiers": "data",
            "signature": "data class Config(val name: String, val limit: Int)",
            "startLine": 2,
            "endLine": 5,
            "startByte": 37,
            "byteLength": 54,
            "childItems": []
        },
        {
            "name": "create",
            "kind": "fun",
            "modifiers": "",
            "signature": "fun create(name: String): Config",
            "startLine": 8,
            "endLine": 10,
            "startByte": 133,
            "byteLength": 64,
            "childItems": []
        },
        {
            "name": "Processor",
            "kind": "class",
            "modifiers": "",
            "startLine": 12,
            "endLine": 17,
            "startByte": 199,
            "byteLength": 107,
            "childItems": [
                {
                    "name": "process",
                    "kind": "fun",
                    "modifiers": "",
                    "signature": "fun process(config: Config): Boolean",
                    "startLine": 14,
                    "endLine": 16,
                    "startByte": 249,
                    "byteLength": 55,
                    "childItems": []
                }
            ]
        },
        {
            "name": "Mode",
            "kind": "enum",
            "modifiers": "",
            "signature": "enum class Mode",
            "startLine": 20,
            "endLine": 23,
            "startByte": 330,
            "byteLength": 51,
            "childItems": [
                {
                    "name": "Fast",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 21,
                    "endLine": 21,
                    "startByte": 350,
                    "byteLength": 4,
                    "childItems": []
                },
                {
                    "name": "Precise",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 22,
                    "endLine": 22,
                    "startByte": 360,
                    "byteLength": 7,
                    "childItems": []
                }
            ]
        },
        {
            "name": "MAX_SIZE",
            "kind": "const",
            "modifiers": "",
            "signature": "const val MAX_SIZE: Int = 1024",
            "startLine": 25,
            "endLine": 25,
            "startByte": 383,
            "byteLength": 30,
            "childItems": []
        }
    ])
}

fn make_adapter() -> KotlinSemanticAdapter<FixtureRuntime> {
    KotlinSemanticAdapter::new(FixtureRuntime)
}

// ---------------------------------------------------------------------------
// Aggregate contract suite
// ---------------------------------------------------------------------------

#[test]
fn kotlin_semantic_passes_all_contracts() {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    contract::run_all_contracts(&adapter, &fixture);
}

// ---------------------------------------------------------------------------
// Individual contract tests (for granular failure diagnostics)
// ---------------------------------------------------------------------------

#[test]
fn contract_adapter_identity_is_stable() {
    let adapter = make_adapter();
    contract::assert_adapter_identity_is_stable(&adapter);
}

#[test]
fn contract_capabilities_are_valid() {
    let adapter = make_adapter();
    contract::assert_capabilities_are_valid(&adapter);
}

#[test]
fn contract_provenance_fields() {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    contract::assert_provenance_fields(&adapter, &fixture);
}

#[test]
fn contract_expected_symbols() {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    contract::assert_expected_symbols(&adapter, &fixture);
}

#[test]
fn contract_symbols_are_valid() {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    contract::assert_symbols_are_valid(&adapter, &fixture);
}

#[test]
fn contract_extraction_is_deterministic() {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    contract::assert_extraction_is_deterministic(&adapter, &fixture);
}

#[test]
fn contract_unsupported_language_rejected() {
    let adapter = make_adapter();
    contract::assert_unsupported_language_rejected(&adapter);
}

#[test]
fn contract_empty_file_produces_no_symbols() {
    let adapter = make_adapter();
    contract::assert_empty_file_produces_no_symbols(&adapter);
}

// ---------------------------------------------------------------------------
// Exact qualified name and symbol ID assertions
// ---------------------------------------------------------------------------

fn extract_fixture_symbols() -> Vec<adapter_api::ExtractedSymbol> {
    let adapter = make_adapter();
    let fixture = ContractFixture::kotlin_baseline();
    let ctx = adapter_api::IndexContext {
        repo_id: "test-repo".to_string(),
        source_root: std::path::PathBuf::from("/tmp/test-repo"),
    };
    let file = adapter_api::SourceFile {
        relative_path: fixture.relative_path.clone(),
        absolute_path: std::path::PathBuf::from("/tmp/test-repo").join(&fixture.relative_path),
        content: fixture.source_code.clone(),
        language: fixture.language.clone(),
    };
    adapter.index_file(&ctx, &file).unwrap().symbols
}

fn find_symbol<'a>(
    symbols: &'a [adapter_api::ExtractedSymbol],
    name: &str,
) -> &'a adapter_api::ExtractedSymbol {
    symbols.iter().find(|s| s.name == name).unwrap_or_else(|| {
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        panic!("symbol '{name}' not found in: {names:?}")
    })
}

#[test]
fn fixture_qualified_names_match_canonical_rules() {
    let symbols = extract_fixture_symbols();

    let config = find_symbol(&symbols, "Config");
    assert_eq!(config.qualified_name, "Config");
    assert!(config.parent_qualified_name.is_none());

    let create = find_symbol(&symbols, "create");
    assert_eq!(create.qualified_name, "create");
    assert!(create.parent_qualified_name.is_none());

    let process = find_symbol(&symbols, "process");
    assert_eq!(process.qualified_name, "Processor::process");
    assert_eq!(process.parent_qualified_name.as_deref(), Some("Processor"));

    let mode = find_symbol(&symbols, "Mode");
    assert_eq!(mode.qualified_name, "Mode");

    let max_size = find_symbol(&symbols, "MAX_SIZE");
    assert_eq!(max_size.qualified_name, "MAX_SIZE");
}

#[test]
fn fixture_symbol_kinds_are_correct() {
    let symbols = extract_fixture_symbols();

    assert_eq!(
        find_symbol(&symbols, "Config").kind,
        core_model::SymbolKind::Class,
        "data class should map to Class"
    );
    assert_eq!(
        find_symbol(&symbols, "create").kind,
        core_model::SymbolKind::Function,
    );
    assert_eq!(
        find_symbol(&symbols, "Processor").kind,
        core_model::SymbolKind::Class,
    );
    assert_eq!(
        find_symbol(&symbols, "process").kind,
        core_model::SymbolKind::Method,
    );
    assert_eq!(
        find_symbol(&symbols, "Mode").kind,
        core_model::SymbolKind::Type,
        "enum should map to Type"
    );
    assert_eq!(
        find_symbol(&symbols, "MAX_SIZE").kind,
        core_model::SymbolKind::Constant,
    );
}

#[test]
fn fixture_symbol_ids_match_expected_canonical_form() {
    let symbols = extract_fixture_symbols();
    let file_path = "src/Config.kt";

    let expected: &[(&str, &str)] = &[
        ("Config", "test-repo//src/Config.kt::Config#class"),
        ("create", "test-repo//src/Config.kt::create#function"),
        ("Processor", "test-repo//src/Config.kt::Processor#class"),
        (
            "process",
            "test-repo//src/Config.kt::Processor::process#method",
        ),
        ("Mode", "test-repo//src/Config.kt::Mode#type"),
        ("MAX_SIZE", "test-repo//src/Config.kt::MAX_SIZE#constant"),
    ];

    for (name, expected_id) in expected {
        let sym = find_symbol(&symbols, name);
        let actual_id = core_model::symbol_id::build_symbol_id(
            "test-repo",
            file_path,
            &sym.qualified_name,
            sym.kind,
        )
        .unwrap_or_else(|e| panic!("symbol '{name}' failed ID construction: {e}"));
        assert_eq!(
            &actual_id, expected_id,
            "symbol '{name}' produced wrong canonical ID"
        );

        core_model::symbol_id::validate_symbol_id(&actual_id)
            .unwrap_or_else(|e| panic!("symbol '{name}' ID '{actual_id}' failed validation: {e}"));
    }
}

#[test]
fn fixture_symbol_ids_are_stable_across_runs() {
    let symbols1 = extract_fixture_symbols();
    let symbols2 = extract_fixture_symbols();
    let file_path = "src/Config.kt";

    assert_eq!(symbols1.len(), symbols2.len());

    for (a, b) in symbols1.iter().zip(symbols2.iter()) {
        let id_a = core_model::symbol_id::build_symbol_id(
            "test-repo",
            file_path,
            &a.qualified_name,
            a.kind,
        )
        .unwrap();
        let id_b = core_model::symbol_id::build_symbol_id(
            "test-repo",
            file_path,
            &b.qualified_name,
            b.kind,
        )
        .unwrap();
        assert_eq!(
            id_a, id_b,
            "symbol ID not stable for '{}': '{}' vs '{}'",
            a.name, id_a, id_b
        );
    }
}

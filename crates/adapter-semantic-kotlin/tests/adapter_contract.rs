//! Contract test suite for the Kotlin semantic backend.
//!
//! Verifies that the Kotlin semantic backend satisfies behavioral
//! contracts for symbol extraction, qualified naming, and determinism.
//!
//! These tests use a mock runtime to avoid requiring a real JVM process.

use semantic_api::SemanticBackend;
use semantic_kotlin::adapter::KotlinSemanticAdapter;
use semantic_kotlin::error::KotlinAnalysisError;
use semantic_kotlin::protocol::KotlinResponse;
use semantic_kotlin::runtime::KotlinRuntime;
use syntax_platform::PreparedFile;

/// A mock runtime that returns a canned analysis response matching the
/// Kotlin baseline fixture.
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
                {"name": "Fast", "kind": "enum_entry", "modifiers": "", "startLine": 21, "endLine": 21, "startByte": 350, "byteLength": 4, "childItems": []},
                {"name": "Precise", "kind": "enum_entry", "modifiers": "", "startLine": 22, "endLine": 22, "startByte": 360, "byteLength": 7, "childItems": []}
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

fn make_fixture_file() -> PreparedFile {
    PreparedFile {
        relative_path: std::path::PathBuf::from("src/Config.kt"),
        absolute_path: std::path::PathBuf::from("/tmp/test-repo/src/Config.kt"),
        content: b"/** Configuration for processing. */\ndata class Config(\n    val name: String,\n    val limit: Int\n)\n\n/** Creates a new config with defaults. */\nfun create(name: String): Config {\n    return Config(name, 100)\n}\n\nclass Processor {\n    /** Processes the config. */\n    fun process(config: Config): Boolean {\n        return config.limit > 0\n    }\n}\n\n/** Operating mode. */\nenum class Mode {\n    Fast,\n    Precise\n}\n\nconst val MAX_SIZE: Int = 1024\n".to_vec(),
        language: "kotlin".to_string(),
    }
}

fn extract_fixture_symbols() -> Vec<semantic_api::SemanticSymbol> {
    let adapter = make_adapter();
    let file = make_fixture_file();
    adapter.enrich_symbols(&file, None).unwrap().symbols
}

fn find_symbol<'a>(
    symbols: &'a [semantic_api::SemanticSymbol],
    name: &str,
) -> &'a semantic_api::SemanticSymbol {
    symbols.iter().find(|s| s.name == name).unwrap_or_else(|| {
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        panic!("symbol '{name}' not found in: {names:?}")
    })
}

// ---------------------------------------------------------------------------
// Backend identity and capability tests
// ---------------------------------------------------------------------------

#[test]
fn backend_identity_is_stable() {
    let adapter = make_adapter();
    assert_eq!(adapter.language(), "kotlin");
    assert_eq!(
        KotlinSemanticAdapter::<FixtureRuntime>::backend_id().0,
        "semantic-kotlin"
    );
}

#[test]
fn capabilities_are_valid() {
    let adapter = make_adapter();
    let cap = adapter.capability();
    assert!(cap.supports_type_refs);
    assert!(cap.supports_call_refs);
    assert!((cap.default_confidence - 0.9).abs() < f32::EPSILON);
}

#[test]
fn rejects_unsupported_language() {
    let adapter = make_adapter();
    let file = PreparedFile {
        language: "python".to_string(),
        ..make_fixture_file()
    };
    let err = adapter
        .enrich_symbols(&file, None)
        .expect_err("should reject");
    assert!(err.to_string().contains("unsupported language"));
}

#[test]
fn empty_file_produces_no_symbols() {
    let adapter = make_adapter();
    let file = PreparedFile {
        content: Vec::new(),
        ..make_fixture_file()
    };
    let output = adapter.enrich_symbols(&file, None).unwrap();
    assert!(output.symbols.is_empty());
}

// ---------------------------------------------------------------------------
// Symbol extraction tests
// ---------------------------------------------------------------------------

#[test]
fn extracts_expected_symbols() {
    let symbols = extract_fixture_symbols();
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in &[
        "Config",
        "create",
        "Processor",
        "process",
        "Mode",
        "MAX_SIZE",
    ] {
        assert!(
            names.contains(expected),
            "expected symbol '{expected}' not found in: {names:?}"
        );
    }
}

#[test]
fn extraction_is_deterministic() {
    let out1 = extract_fixture_symbols();
    let out2 = extract_fixture_symbols();

    assert_eq!(out1.len(), out2.len());
    for (a, b) in out1.iter().zip(out2.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.qualified_name, b.qualified_name);
        assert_eq!(a.span, b.span);
        assert_eq!(a.confidence_score, b.confidence_score);
    }
}

#[test]
fn provenance_fields_are_correct() {
    let adapter = make_adapter();
    let file = make_fixture_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    assert_eq!(output.backend_id.0, "semantic-kotlin");
    assert_eq!(output.language, "kotlin");
}

// ---------------------------------------------------------------------------
// Exact qualified name and symbol ID assertions
// ---------------------------------------------------------------------------

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
        core_model::SymbolKind::Class
    );
    assert_eq!(
        find_symbol(&symbols, "create").kind,
        core_model::SymbolKind::Function
    );
    assert_eq!(
        find_symbol(&symbols, "Processor").kind,
        core_model::SymbolKind::Class
    );
    assert_eq!(
        find_symbol(&symbols, "process").kind,
        core_model::SymbolKind::Method
    );
    assert_eq!(
        find_symbol(&symbols, "Mode").kind,
        core_model::SymbolKind::Type
    );
    assert_eq!(
        find_symbol(&symbols, "MAX_SIZE").kind,
        core_model::SymbolKind::Constant
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
        assert_eq!(id_a, id_b, "symbol ID not stable for '{}'", a.name);
    }
}

//! Contract test suite for the TypeScript semantic backend.
//!
//! Verifies that the TypeScript semantic backend satisfies behavioral
//! contracts for symbol extraction, qualified naming, and determinism.
//!
//! These tests use a mock runtime to avoid requiring a real tsserver
//! process. The mock returns navtree responses that match the fixture
//! source code, verifying the mapping logic end-to-end.

use semantic_api::SemanticBackend;
use semantic_typescript::adapter::TypeScriptSemanticAdapter;
use semantic_typescript::error::TsServerError;
use semantic_typescript::protocol::TsServerResponse;
use semantic_typescript::runtime::SemanticRuntime;
use syntax_platform::PreparedFile;

/// A mock runtime that returns a canned navtree response matching the
/// TypeScript baseline fixture.
struct FixtureRuntime;

impl SemanticRuntime for FixtureRuntime {
    fn start(&mut self) -> Result<(), TsServerError> {
        Ok(())
    }

    fn stop(&mut self) {}

    fn restart(&mut self) -> Result<(), TsServerError> {
        Ok(())
    }

    fn is_healthy(&mut self) -> bool {
        true
    }

    fn send_request(
        &mut self,
        command: &str,
        _arguments: Option<serde_json::Value>,
    ) -> Result<TsServerResponse, TsServerError> {
        match command {
            "navtree" => Ok(TsServerResponse {
                seq: 0,
                msg_type: "response".to_string(),
                command: Some("navtree".to_string()),
                request_seq: Some(1),
                success: Some(true),
                body: Some(fixture_navtree_body()),
                message: None,
            }),
            _ => Ok(TsServerResponse {
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

/// Returns a navtree response body that matches the TypeScript baseline fixture.
fn fixture_navtree_body() -> serde_json::Value {
    serde_json::json!({
        "text": "<global>",
        "kind": "script",
        "kindModifiers": "",
        "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 27, "offset": 1}}],
        "childItems": [
            {
                "text": "Config",
                "kind": "interface",
                "kindModifiers": "",
                "spans": [{"start": {"line": 2, "offset": 1}, "end": {"line": 5, "offset": 2}}],
                "childItems": [
                    {
                        "text": "name",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 3, "offset": 5}, "end": {"line": 3, "offset": 18}}],
                        "childItems": []
                    },
                    {
                        "text": "limit",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 4, "offset": 5}, "end": {"line": 4, "offset": 19}}],
                        "childItems": []
                    }
                ]
            },
            {
                "text": "create",
                "kind": "function",
                "kindModifiers": "",
                "spans": [{"start": {"line": 8, "offset": 1}, "end": {"line": 10, "offset": 2}}],
                "childItems": []
            },
            {
                "text": "Processor",
                "kind": "class",
                "kindModifiers": "",
                "spans": [{"start": {"line": 12, "offset": 1}, "end": {"line": 17, "offset": 2}}],
                "childItems": [
                    {
                        "text": "process",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 14, "offset": 5}, "end": {"line": 16, "offset": 6}}],
                        "childItems": []
                    }
                ]
            },
            {
                "text": "Mode",
                "kind": "enum",
                "kindModifiers": "",
                "spans": [{"start": {"line": 20, "offset": 1}, "end": {"line": 23, "offset": 2}}],
                "childItems": [
                    {
                        "text": "Fast",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 21, "offset": 5}, "end": {"line": 21, "offset": 9}}],
                        "childItems": []
                    },
                    {
                        "text": "Precise",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 22, "offset": 5}, "end": {"line": 22, "offset": 12}}],
                        "childItems": []
                    }
                ]
            },
            {
                "text": "MAX_SIZE",
                "kind": "const",
                "kindModifiers": "",
                "spans": [{"start": {"line": 25, "offset": 1}, "end": {"line": 25, "offset": 28}}],
                "childItems": []
            }
        ]
    })
}

fn make_adapter() -> TypeScriptSemanticAdapter<FixtureRuntime> {
    TypeScriptSemanticAdapter::new(FixtureRuntime)
}

const FIXTURE_SOURCE: &str = "/** Configuration for processing. */\ninterface Config {\n    name: string;\n    limit: number;\n}\n\n/** Creates a new config with defaults. */\nfunction create(name: string): Config {\n    return { name, limit: 100 };\n}\n\nclass Processor {\n    /** Processes the config. */\n    process(config: Config): boolean {\n        return config.limit > 0;\n    }\n}\n\n/** Operating mode. */\nenum Mode {\n    Fast,\n    Precise,\n}\n\nconst MAX_SIZE: number = 1024;\n";

fn make_fixture_file() -> PreparedFile {
    PreparedFile {
        relative_path: std::path::PathBuf::from("src/config.ts"),
        absolute_path: std::path::PathBuf::from("/tmp/test-repo/src/config.ts"),
        content: FIXTURE_SOURCE.as_bytes().to_vec(),
        language: "typescript".to_string(),
    }
}

/// Helper: extracts symbols from the TypeScript baseline fixture.
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
    assert_eq!(adapter.language(), "typescript");
    assert_eq!(
        TypeScriptSemanticAdapter::<FixtureRuntime>::backend_id().0,
        "semantic-typescript"
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
    let adapter = make_adapter();
    let file = make_fixture_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
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

    assert_eq!(output.backend_id.0, "semantic-typescript");
    assert_eq!(output.language, "typescript");
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
        core_model::SymbolKind::Type
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
    let file_path = "src/config.ts";

    let expected: &[(&str, &str)] = &[
        ("Config", "test-repo//src/config.ts::Config#type"),
        ("create", "test-repo//src/config.ts::create#function"),
        ("Processor", "test-repo//src/config.ts::Processor#class"),
        (
            "process",
            "test-repo//src/config.ts::Processor::process#method",
        ),
        ("Mode", "test-repo//src/config.ts::Mode#type"),
        ("MAX_SIZE", "test-repo//src/config.ts::MAX_SIZE#constant"),
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
    let file_path = "src/config.ts";

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

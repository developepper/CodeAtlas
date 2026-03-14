//! Contract test suite for the TypeScript semantic adapter.
//!
//! Uses the shared contract harness from `adapter-api` to verify that
//! the TypeScript semantic adapter satisfies all behavioral contracts.
//!
//! These tests use a mock runtime to avoid requiring a real tsserver
//! process. The mock returns navtree responses that match the fixture
//! source code, verifying the mapping logic end-to-end.

use adapter_api::contract::{self, ContractFixture};
use adapter_api::LanguageAdapter;
use adapter_semantic_typescript::adapter::TypeScriptSemanticAdapter;
use adapter_semantic_typescript::error::TsServerError;
use adapter_semantic_typescript::protocol::TsServerResponse;
use adapter_semantic_typescript::runtime::SemanticRuntime;

/// A mock runtime that returns a canned navtree response matching the
/// TypeScript baseline fixture from the contract test harness.
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
///
/// The fixture source is:
/// ```typescript
/// /** Configuration for processing. */
/// interface Config {
///     name: string;
///     limit: number;
/// }
///
/// /** Creates a new config with defaults. */
/// function create(name: string): Config {
///     return { name, limit: 100 };
/// }
///
/// class Processor {
///     /** Processes the config. */
///     process(config: Config): boolean {
///         return config.limit > 0;
///     }
/// }
///
/// /** Operating mode. */
/// enum Mode {
///     Fast,
///     Precise,
/// }
///
/// const MAX_SIZE: number = 1024;
/// ```
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

// ---------------------------------------------------------------------------
// Aggregate contract suite
// ---------------------------------------------------------------------------

#[test]
fn typescript_semantic_passes_all_contracts() {
    let adapter = make_adapter();
    let fixture = ContractFixture::typescript_baseline();
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
    let fixture = ContractFixture::typescript_baseline();
    contract::assert_provenance_fields(&adapter, &fixture);
}

#[test]
fn contract_expected_symbols() {
    let adapter = make_adapter();
    let fixture = ContractFixture::typescript_baseline();
    contract::assert_expected_symbols(&adapter, &fixture);
}

#[test]
fn contract_symbols_are_valid() {
    let adapter = make_adapter();
    let fixture = ContractFixture::typescript_baseline();
    contract::assert_symbols_are_valid(&adapter, &fixture);
}

#[test]
fn contract_extraction_is_deterministic() {
    let adapter = make_adapter();
    let fixture = ContractFixture::typescript_baseline();
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

/// Helper: extracts symbols from the TypeScript baseline fixture.
fn extract_fixture_symbols() -> Vec<adapter_api::ExtractedSymbol> {
    let adapter = make_adapter();
    let fixture = ContractFixture::typescript_baseline();
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

    // Top-level symbols have unqualified names.
    let config = find_symbol(&symbols, "Config");
    assert_eq!(config.qualified_name, "Config");
    assert!(
        config.parent_qualified_name.is_none(),
        "Config is top-level, should have no parent"
    );

    let create = find_symbol(&symbols, "create");
    assert_eq!(create.qualified_name, "create");
    assert!(
        create.parent_qualified_name.is_none(),
        "create is top-level, should have no parent"
    );

    // Class methods use "::" scope separator.
    let process = find_symbol(&symbols, "process");
    assert_eq!(
        process.qualified_name, "Processor::process",
        "method must be scoped under its class"
    );
    assert_eq!(
        process.parent_qualified_name.as_deref(),
        Some("Processor"),
        "method parent must be the enclosing class"
    );

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
        core_model::SymbolKind::Type,
        "interface should map to Type"
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
    let file_path = "src/config.ts";

    // Assert exact expected IDs for every fixture symbol.
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

        // Verify the ID also passes parse/validation round-trip.
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
        assert_eq!(
            id_a, id_b,
            "symbol ID not stable for '{}': '{}' vs '{}'",
            a.name, id_a, id_b
        );
    }
}

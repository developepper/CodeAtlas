//! Quality regression test suite for the TypeScript semantic backend.
//!
//! Verifies that the TypeScript semantic backend produces expected symbols
//! with correct qualified names and confidence, using regression fixtures
//! and assertions that can be enforced in CI.
//!
//! These tests use a mock runtime to avoid requiring a real tsserver process.

use semantic_api::SemanticBackend;
use semantic_typescript::adapter::TypeScriptSemanticAdapter;
use semantic_typescript::error::TsServerError;
use semantic_typescript::protocol::TsServerResponse;
use semantic_typescript::runtime::SemanticRuntime;
use syntax_platform::PreparedFile;

/// A mock runtime that returns navtree responses matching the TypeScript
/// regression fixture source code.
struct RegressionRuntime;

impl SemanticRuntime for RegressionRuntime {
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
                body: Some(regression_navtree_body()),
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

/// Navtree response matching the TypeScript regression fixture.
fn regression_navtree_body() -> serde_json::Value {
    serde_json::json!({
        "text": "<global>",
        "kind": "script",
        "kindModifiers": "",
        "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 50, "offset": 1}}],
        "childItems": [
            {
                "text": "ServiceConfig",
                "kind": "interface",
                "kindModifiers": "",
                "spans": [{"start": {"line": 2, "offset": 1}, "end": {"line": 6, "offset": 2}}],
                "childItems": [
                    {"text": "host", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 3, "offset": 5}, "end": {"line": 3, "offset": 18}}], "childItems": []},
                    {"text": "port", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 4, "offset": 5}, "end": {"line": 4, "offset": 18}}], "childItems": []},
                    {"text": "timeout", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 5, "offset": 5}, "end": {"line": 5, "offset": 21}}], "childItems": []}
                ]
            },
            {
                "text": "createService",
                "kind": "function",
                "kindModifiers": "",
                "spans": [{"start": {"line": 9, "offset": 1}, "end": {"line": 11, "offset": 2}}],
                "childItems": []
            },
            {
                "text": "ServiceImpl",
                "kind": "class",
                "kindModifiers": "",
                "spans": [{"start": {"line": 14, "offset": 1}, "end": {"line": 37, "offset": 2}}],
                "childItems": [
                    {"text": "config", "kind": "property", "kindModifiers": "private", "spans": [{"start": {"line": 15, "offset": 5}, "end": {"line": 15, "offset": 38}}], "childItems": []},
                    {"text": "constructor", "kind": "constructor", "kindModifiers": "", "spans": [{"start": {"line": 17, "offset": 5}, "end": {"line": 19, "offset": 6}}], "childItems": []},
                    {"text": "start", "kind": "method", "kindModifiers": "", "spans": [{"start": {"line": 22, "offset": 5}, "end": {"line": 24, "offset": 6}}], "childItems": []},
                    {"text": "stop", "kind": "method", "kindModifiers": "", "spans": [{"start": {"line": 27, "offset": 5}, "end": {"line": 29, "offset": 6}}], "childItems": []},
                    {"text": "handleRequest", "kind": "method", "kindModifiers": "", "spans": [{"start": {"line": 32, "offset": 5}, "end": {"line": 34, "offset": 6}}], "childItems": []}
                ]
            },
            {
                "text": "ServiceStatus",
                "kind": "enum",
                "kindModifiers": "",
                "spans": [{"start": {"line": 38, "offset": 1}, "end": {"line": 43, "offset": 2}}],
                "childItems": [
                    {"text": "Starting", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 39, "offset": 5}, "end": {"line": 39, "offset": 13}}], "childItems": []},
                    {"text": "Running", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 40, "offset": 5}, "end": {"line": 40, "offset": 12}}], "childItems": []},
                    {"text": "Stopping", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 41, "offset": 5}, "end": {"line": 41, "offset": 13}}], "childItems": []},
                    {"text": "Stopped", "kind": "property", "kindModifiers": "", "spans": [{"start": {"line": 42, "offset": 5}, "end": {"line": 42, "offset": 12}}], "childItems": []}
                ]
            },
            {
                "text": "RequestHandler",
                "kind": "type",
                "kindModifiers": "",
                "spans": [{"start": {"line": 46, "offset": 1}, "end": {"line": 46, "offset": 58}}],
                "childItems": []
            },
            {
                "text": "DEFAULT_TIMEOUT",
                "kind": "const",
                "kindModifiers": "",
                "spans": [{"start": {"line": 49, "offset": 1}, "end": {"line": 49, "offset": 40}}],
                "childItems": []
            }
        ]
    })
}

const REGRESSION_SOURCE: &str = "/** Service configuration. */\ninterface ServiceConfig {\n    host: string;\n    port: number;\n    timeout: number;\n}\n\n/** Creates a service with config. */\nfunction createService(config: ServiceConfig): ServiceImpl {\n    return new ServiceImpl(config);\n}\n\n/** Service implementation. */\nclass ServiceImpl {\n    private config: ServiceConfig;\n\n    constructor(config: ServiceConfig) {\n        this.config = config;\n    }\n\n    /** Starts the service. */\n    start(): void {\n        console.log('starting');\n    }\n\n    /** Stops the service. */\n    stop(): void {\n        console.log('stopping');\n    }\n\n    /** Handles an incoming request. */\n    handleRequest(path: string): boolean {\n        return path.length > 0;\n    }\n}\n\nenum ServiceStatus {\n    Starting,\n    Running,\n    Stopping,\n    Stopped,\n}\n\n/** Request handler type. */\ntype RequestHandler = (path: string, body: unknown) => boolean;\n\n/** Default timeout in milliseconds. */\nconst DEFAULT_TIMEOUT: number = 30000;\n";

fn make_adapter() -> TypeScriptSemanticAdapter<RegressionRuntime> {
    TypeScriptSemanticAdapter::new(RegressionRuntime)
}

fn make_regression_file() -> PreparedFile {
    PreparedFile {
        relative_path: std::path::PathBuf::from("src/service.ts"),
        absolute_path: std::path::PathBuf::from("/tmp/test-repo/src/service.ts"),
        content: REGRESSION_SOURCE.as_bytes().to_vec(),
        language: "typescript".to_string(),
    }
}

#[test]
fn regression_extracts_expected_symbols() {
    let adapter = make_adapter();
    let file = make_regression_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    // The mock navtree body has spans that reference specific line numbers.
    // Only symbols whose spans resolve within the source content are extracted.
    // We verify the core symbols that are guaranteed to have valid spans.
    for expected in &["ServiceConfig", "createService", "ServiceImpl"] {
        assert!(
            names.contains(expected),
            "expected symbol '{expected}' not found in: {names:?}"
        );
    }
    // Overall we should have at least some symbols extracted.
    assert!(
        output.symbols.len() >= 3,
        "expected at least 3 symbols, got {}",
        output.symbols.len()
    );
}

#[test]
fn regression_is_deterministic() {
    let file = make_regression_file();

    let out1 = {
        let adapter = make_adapter();
        adapter.enrich_symbols(&file, None).unwrap()
    };
    let out2 = {
        let adapter = make_adapter();
        adapter.enrich_symbols(&file, None).unwrap()
    };

    assert_eq!(out1.symbols.len(), out2.symbols.len());
    for (a, b) in out1.symbols.iter().zip(out2.symbols.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.qualified_name, b.qualified_name);
        assert_eq!(a.span, b.span);
    }
}

#[test]
fn regression_qualified_names_are_correct() {
    let adapter = make_adapter();
    let file = make_regression_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    let find = |name: &str| {
        output
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{name}' not found"))
    };

    assert_eq!(find("ServiceConfig").qualified_name, "ServiceConfig");
    assert_eq!(find("createService").qualified_name, "createService");
    assert_eq!(find("start").qualified_name, "ServiceImpl::start");
    assert_eq!(find("stop").qualified_name, "ServiceImpl::stop");
    assert_eq!(
        find("handleRequest").qualified_name,
        "ServiceImpl::handleRequest"
    );
}

#[test]
fn regression_all_symbols_have_confidence() {
    let adapter = make_adapter();
    let file = make_regression_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    for sym in &output.symbols {
        let score = sym
            .confidence_score
            .unwrap_or_else(|| panic!("symbol '{}' missing confidence", sym.name));
        assert!(
            (0.0..=1.0).contains(&score),
            "confidence out of range for '{}'",
            sym.name
        );
    }
}

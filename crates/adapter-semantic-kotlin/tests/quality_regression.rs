//! Quality regression test suite for the Kotlin semantic backend.
//!
//! Verifies that the Kotlin semantic backend produces expected symbols
//! with correct qualified names and confidence, using regression fixtures.
//!
//! These tests use a mock runtime to avoid requiring a real JVM process.

use semantic_api::SemanticBackend;
use semantic_kotlin::adapter::KotlinSemanticAdapter;
use semantic_kotlin::error::KotlinAnalysisError;
use semantic_kotlin::protocol::KotlinResponse;
use semantic_kotlin::runtime::KotlinRuntime;
use syntax_platform::PreparedFile;

/// A mock runtime that returns analysis responses matching the Kotlin
/// regression fixture source code.
struct RegressionRuntime;

impl KotlinRuntime for RegressionRuntime {
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
                body: Some(regression_analysis_body()),
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

fn regression_analysis_body() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "ServiceConfig",
            "kind": "class",
            "modifiers": "data",
            "signature": "data class ServiceConfig(val host: String, val port: Int, val timeout: Long)",
            "startLine": 2,
            "endLine": 6,
            "startByte": 42,
            "byteLength": 75,
            "childItems": []
        },
        {
            "name": "createService",
            "kind": "fun",
            "modifiers": "",
            "signature": "fun createService(config: ServiceConfig): ServiceImpl",
            "startLine": 9,
            "endLine": 11,
            "startByte": 160,
            "byteLength": 82,
            "childItems": []
        },
        {
            "name": "ServiceImpl",
            "kind": "class",
            "modifiers": "",
            "signature": "class ServiceImpl(private val config: ServiceConfig)",
            "startLine": 14,
            "endLine": 29,
            "startByte": 286,
            "byteLength": 380,
            "childItems": [
                {"name": "start", "kind": "fun", "modifiers": "", "signature": "fun start()", "startLine": 16, "endLine": 18, "startByte": 365, "byteLength": 75, "childItems": []},
                {"name": "stop", "kind": "fun", "modifiers": "", "signature": "fun stop()", "startLine": 21, "endLine": 23, "startByte": 478, "byteLength": 55, "childItems": []},
                {"name": "handleRequest", "kind": "fun", "modifiers": "", "signature": "fun handleRequest(path: String, body: Any?): Boolean", "startLine": 26, "endLine": 28, "startByte": 575, "byteLength": 85, "childItems": []}
            ]
        },
        {
            "name": "ServiceStatus",
            "kind": "enum",
            "modifiers": "",
            "signature": "enum class ServiceStatus",
            "startLine": 32,
            "endLine": 37,
            "startByte": 710,
            "byteLength": 80,
            "childItems": [
                {"name": "Starting", "kind": "enum_entry", "modifiers": "", "startLine": 33, "endLine": 33, "startByte": 740, "byteLength": 8, "childItems": []},
                {"name": "Running", "kind": "enum_entry", "modifiers": "", "startLine": 34, "endLine": 34, "startByte": 754, "byteLength": 7, "childItems": []},
                {"name": "Stopping", "kind": "enum_entry", "modifiers": "", "startLine": 35, "endLine": 35, "startByte": 767, "byteLength": 8, "childItems": []},
                {"name": "Stopped", "kind": "enum_entry", "modifiers": "", "startLine": 36, "endLine": 36, "startByte": 781, "byteLength": 7, "childItems": []}
            ]
        },
        {
            "name": "RequestHandler",
            "kind": "typealias",
            "modifiers": "",
            "signature": "typealias RequestHandler = (String, Any?) -> Boolean",
            "startLine": 40,
            "endLine": 40,
            "startByte": 840,
            "byteLength": 53,
            "childItems": []
        },
        {
            "name": "DEFAULT_TIMEOUT",
            "kind": "const",
            "modifiers": "",
            "signature": "const val DEFAULT_TIMEOUT: Long = 30000",
            "startLine": 43,
            "endLine": 43,
            "startByte": 940,
            "byteLength": 39,
            "childItems": []
        }
    ])
}

fn make_adapter() -> KotlinSemanticAdapter<RegressionRuntime> {
    KotlinSemanticAdapter::new(RegressionRuntime)
}

fn make_regression_file() -> PreparedFile {
    PreparedFile {
        relative_path: std::path::PathBuf::from("src/Service.kt"),
        absolute_path: std::path::PathBuf::from("/tmp/test-repo/src/Service.kt"),
        content: b"/** Service configuration. */\ndata class ServiceConfig(\n    val host: String,\n    val port: Int,\n    val timeout: Long\n)\n\nfun createService(config: ServiceConfig): ServiceImpl {\n    return ServiceImpl(config)\n}\n\nclass ServiceImpl(private val config: ServiceConfig) {\n    fun start() { println(\"starting\") }\n    fun stop() { println(\"stopping\") }\n    fun handleRequest(path: String, body: Any?): Boolean { return path.isNotEmpty() }\n}\n\nenum class ServiceStatus {\n    Starting, Running, Stopping, Stopped\n}\n\ntypealias RequestHandler = (String, Any?) -> Boolean\n\nconst val DEFAULT_TIMEOUT: Long = 30000\n".to_vec(),
        language: "kotlin".to_string(),
    }
}

#[test]
fn regression_extracts_expected_symbols() {
    let adapter = make_adapter();
    let file = make_regression_file();
    let output = adapter.enrich_symbols(&file, None).unwrap();

    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in &[
        "ServiceConfig",
        "createService",
        "ServiceImpl",
        "start",
        "stop",
        "handleRequest",
        "ServiceStatus",
        "RequestHandler",
        "DEFAULT_TIMEOUT",
    ] {
        assert!(
            names.contains(expected),
            "expected symbol '{expected}' not found in: {names:?}"
        );
    }
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

//! Quality regression test suite for the Kotlin semantic adapter.
//!
//! Verifies that the Kotlin semantic adapter maintains measurable
//! quality improvements over a syntax baseline, using regression fixtures
//! and KPI thresholds that are enforced in CI.
//!
//! These tests use a mock runtime to avoid requiring a real JVM process.

use adapter_api::regression::{self, RegressionFixture};
use adapter_semantic_kotlin::adapter::KotlinSemanticAdapter;
use adapter_semantic_kotlin::error::KotlinAnalysisError;
use adapter_semantic_kotlin::protocol::KotlinResponse;
use adapter_semantic_kotlin::runtime::KotlinRuntime;

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

/// Analysis response matching the Kotlin regression fixture.
///
/// The fixture source defines:
/// - `ServiceConfig` (data class)
/// - `createService` (top-level function)
/// - `ServiceImpl` (class with start, stop, handleRequest methods)
/// - `ServiceStatus` (enum class)
/// - `RequestHandler` (typealias)
/// - `DEFAULT_TIMEOUT` (const val)
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
                {
                    "name": "start",
                    "kind": "fun",
                    "modifiers": "",
                    "signature": "fun start()",
                    "startLine": 16,
                    "endLine": 18,
                    "startByte": 365,
                    "byteLength": 75,
                    "childItems": []
                },
                {
                    "name": "stop",
                    "kind": "fun",
                    "modifiers": "",
                    "signature": "fun stop()",
                    "startLine": 21,
                    "endLine": 23,
                    "startByte": 478,
                    "byteLength": 55,
                    "childItems": []
                },
                {
                    "name": "handleRequest",
                    "kind": "fun",
                    "modifiers": "",
                    "signature": "fun handleRequest(path: String, body: Any?): Boolean",
                    "startLine": 26,
                    "endLine": 28,
                    "startByte": 575,
                    "byteLength": 85,
                    "childItems": []
                }
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
                {
                    "name": "Starting",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 33,
                    "endLine": 33,
                    "startByte": 740,
                    "byteLength": 8,
                    "childItems": []
                },
                {
                    "name": "Running",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 34,
                    "endLine": 34,
                    "startByte": 754,
                    "byteLength": 7,
                    "childItems": []
                },
                {
                    "name": "Stopping",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 35,
                    "endLine": 35,
                    "startByte": 767,
                    "byteLength": 8,
                    "childItems": []
                },
                {
                    "name": "Stopped",
                    "kind": "enum_entry",
                    "modifiers": "",
                    "startLine": 36,
                    "endLine": 36,
                    "startByte": 781,
                    "byteLength": 7,
                    "childItems": []
                }
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

// ---------------------------------------------------------------------------
// Quality regression tests
// ---------------------------------------------------------------------------

#[test]
fn kotlin_quality_regression_passes_thresholds() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    let result = regression::run_quality_regression(&adapter, &fixture);
    result.assert_thresholds();
}

#[test]
fn kotlin_quality_regression_is_deterministic() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    regression::assert_regression_is_deterministic(&adapter, &fixture);
}

#[test]
fn kotlin_regression_win_rate_is_total() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    let result = regression::run_quality_regression(&adapter, &fixture);

    assert_eq!(
        result.kpi.losses, 0,
        "semantic adapter must not lose to syntax on any symbol"
    );
    assert!(
        result.kpi.wins > 0,
        "semantic adapter must have at least one win over syntax"
    );
}

#[test]
fn kotlin_regression_extracts_expected_symbols() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    let result = regression::run_quality_regression(&adapter, &fixture);

    let names: Vec<&str> = result
        .output
        .symbols
        .iter()
        .map(|s| s.name.as_str())
        .collect();

    for expected in &fixture.expected_symbols {
        assert!(
            names.contains(&expected.name.as_str()),
            "expected symbol '{}' not found in regression output: {:?}",
            expected.name,
            names
        );
    }
}

#[test]
fn kotlin_regression_qualified_names_are_correct() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    let result = regression::run_quality_regression(&adapter, &fixture);

    for expected in &fixture.expected_symbols {
        if let Some(sym) = result
            .output
            .symbols
            .iter()
            .find(|s| s.name == expected.name)
        {
            assert_eq!(
                sym.qualified_name, expected.qualified_name,
                "qualified name mismatch for '{}'",
                expected.name
            );
        }
    }
}

#[test]
fn kotlin_regression_report_is_generated() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::kotlin();
    let result = regression::run_quality_regression(&adapter, &fixture);
    let report = result.report();

    // Print the report so CI can capture it with --nocapture.
    println!("\n{report}\n");

    assert!(report.contains("Quality Regression Report"));
    assert!(report.contains("Win rate"));
}

//! Quality regression test suite for the TypeScript semantic adapter.
//!
//! Verifies that the TypeScript semantic adapter maintains measurable
//! quality improvements over a syntax baseline, using regression fixtures
//! and KPI thresholds that are enforced in CI.
//!
//! These tests use a mock runtime to avoid requiring a real tsserver process.

use adapter_api::regression::{self, RegressionFixture};
use adapter_semantic_typescript::adapter::TypeScriptSemanticAdapter;
use adapter_semantic_typescript::error::TsServerError;
use adapter_semantic_typescript::protocol::TsServerResponse;
use adapter_semantic_typescript::runtime::SemanticRuntime;

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
///
/// The fixture source defines:
/// - `ServiceConfig` (interface)
/// - `createService` (function)
/// - `ServiceImpl` (class with start, stop, handleRequest methods)
/// - `ServiceStatus` (enum)
/// - `RequestHandler` (type alias)
/// - `DEFAULT_TIMEOUT` (const)
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
                    {
                        "text": "host",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 3, "offset": 5}, "end": {"line": 3, "offset": 18}}],
                        "childItems": []
                    },
                    {
                        "text": "port",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 4, "offset": 5}, "end": {"line": 4, "offset": 18}}],
                        "childItems": []
                    },
                    {
                        "text": "timeout",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 5, "offset": 5}, "end": {"line": 5, "offset": 21}}],
                        "childItems": []
                    }
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
                    {
                        "text": "config",
                        "kind": "property",
                        "kindModifiers": "private",
                        "spans": [{"start": {"line": 15, "offset": 5}, "end": {"line": 15, "offset": 38}}],
                        "childItems": []
                    },
                    {
                        "text": "constructor",
                        "kind": "constructor",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 17, "offset": 5}, "end": {"line": 19, "offset": 6}}],
                        "childItems": []
                    },
                    {
                        "text": "start",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 22, "offset": 5}, "end": {"line": 24, "offset": 6}}],
                        "childItems": []
                    },
                    {
                        "text": "stop",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 27, "offset": 5}, "end": {"line": 29, "offset": 6}}],
                        "childItems": []
                    },
                    {
                        "text": "handleRequest",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 32, "offset": 5}, "end": {"line": 34, "offset": 6}}],
                        "childItems": []
                    }
                ]
            },
            {
                "text": "ServiceStatus",
                "kind": "enum",
                "kindModifiers": "",
                "spans": [{"start": {"line": 38, "offset": 1}, "end": {"line": 43, "offset": 2}}],
                "childItems": [
                    {
                        "text": "Starting",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 39, "offset": 5}, "end": {"line": 39, "offset": 13}}],
                        "childItems": []
                    },
                    {
                        "text": "Running",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 40, "offset": 5}, "end": {"line": 40, "offset": 12}}],
                        "childItems": []
                    },
                    {
                        "text": "Stopping",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 41, "offset": 5}, "end": {"line": 41, "offset": 13}}],
                        "childItems": []
                    },
                    {
                        "text": "Stopped",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 42, "offset": 5}, "end": {"line": 42, "offset": 12}}],
                        "childItems": []
                    }
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

fn make_adapter() -> TypeScriptSemanticAdapter<RegressionRuntime> {
    TypeScriptSemanticAdapter::new(RegressionRuntime)
}

// ---------------------------------------------------------------------------
// Quality regression tests
// ---------------------------------------------------------------------------

#[test]
fn typescript_quality_regression_passes_thresholds() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
    let result = regression::run_quality_regression(&adapter, &fixture);
    result.assert_thresholds();
}

#[test]
fn typescript_quality_regression_is_deterministic() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
    regression::assert_regression_is_deterministic(&adapter, &fixture);
}

#[test]
fn typescript_regression_win_rate_is_total() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
    let result = regression::run_quality_regression(&adapter, &fixture);

    // Semantic adapter should win on every overlapping symbol.
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
fn typescript_regression_extracts_expected_symbols() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
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
fn typescript_regression_qualified_names_are_correct() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
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
fn typescript_regression_report_is_generated() {
    let adapter = make_adapter();
    let fixture = RegressionFixture::typescript();
    let result = regression::run_quality_regression(&adapter, &fixture);
    let report = result.report();

    // Print the report so CI can capture it with --nocapture.
    println!("\n{report}\n");

    assert!(report.contains("Quality Regression Report"));
    assert!(report.contains("Win rate"));
}

use std::cell::RefCell;

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, IndexContext, LanguageAdapter, SourceFile,
};
use core_model::QualityLevel;
use tracing::{debug, warn};

use crate::error::TsServerError;
use crate::mapping::{map_navtree_to_symbols, NavTreeItem};
use crate::runtime::SemanticRuntime;

const ADAPTER_ID: &str = "semantic-typescript-v1";
const LANGUAGE: &str = "typescript";

/// A semantic adapter for TypeScript that uses tsserver's navigation tree
/// to extract symbols with high-confidence, type-aware metadata.
///
/// The adapter communicates with a tsserver process through the
/// [`SemanticRuntime`] trait, enabling test doubles without real processes.
pub struct TypeScriptSemanticAdapter<R: SemanticRuntime> {
    runtime: RefCell<R>,
    capabilities: AdapterCapabilities,
}

impl<R: SemanticRuntime> TypeScriptSemanticAdapter<R> {
    /// Creates a new TypeScript semantic adapter wrapping the given runtime.
    ///
    /// The runtime must be started before calling `index_file`. The adapter
    /// does not manage the runtime lifecycle — callers are responsible for
    /// starting and stopping the runtime.
    pub fn new(runtime: R) -> Self {
        Self {
            runtime: RefCell::new(runtime),
            capabilities: AdapterCapabilities::semantic_baseline(),
        }
    }

    /// Sends an "open" request to tsserver for the given file, then requests
    /// the navigation tree and parses the response into `NavTreeItem`s.
    fn request_navtree(&self, file: &SourceFile) -> Result<Vec<NavTreeItem>, AdapterError> {
        let file_path = file.absolute_path.to_string_lossy().to_string();
        let content = String::from_utf8_lossy(&file.content).to_string();
        let mut rt = self.runtime.borrow_mut();

        // Open the file in tsserver with its content.
        let open_args = serde_json::json!({
            "file": file_path,
            "fileContent": content,
            "scriptKindName": script_kind_for_path(&file.relative_path),
        });

        rt.send_request("open", Some(open_args)).map_err(|e| {
            warn!(file = %file.relative_path.display(), error = %e, "tsserver open failed");
            ts_error_to_adapter_error(e, &file.relative_path)
        })?;

        // Request the navigation tree.
        let navtree_args = serde_json::json!({
            "file": file_path,
        });

        let response = rt
            .send_request("navtree", Some(navtree_args))
            .map_err(|e| {
                warn!(file = %file.relative_path.display(), error = %e, "tsserver navtree failed");
                ts_error_to_adapter_error(e, &file.relative_path)
            })?;

        // Close the file to free tsserver resources.
        let close_args = serde_json::json!({ "file": file_path });
        if let Err(e) = rt.send_request("close", Some(close_args)) {
            debug!(file = %file.relative_path.display(), error = %e, "tsserver close failed (non-fatal)");
        }

        // Parse the navtree from the response body.
        let body = response.body.ok_or_else(|| AdapterError::Parse {
            path: file.relative_path.clone(),
            reason: "navtree response has no body".to_string(),
        })?;

        parse_navtree_body(&body, &file.relative_path)
    }
}

impl<R: SemanticRuntime> LanguageAdapter for TypeScriptSemanticAdapter<R> {
    fn adapter_id(&self) -> &str {
        ADAPTER_ID
    }

    fn language(&self) -> &str {
        LANGUAGE
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        &self.capabilities
    }

    fn index_file(
        &self,
        _ctx: &IndexContext,
        file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        if file.language != LANGUAGE {
            return Err(AdapterError::Unsupported {
                language: file.language.clone(),
            });
        }

        // Empty files produce no symbols.
        if file.content.is_empty() {
            return Ok(AdapterOutput {
                symbols: vec![],
                source_adapter: ADAPTER_ID.to_string(),
                quality_level: QualityLevel::Semantic,
            });
        }

        let navtree_items = self.request_navtree(file)?;
        let mut symbols = map_navtree_to_symbols(&navtree_items, &file.content);

        // Apply default confidence to symbols that don't have per-symbol overrides.
        let default_confidence = self.capabilities.default_confidence;
        for sym in &mut symbols {
            if sym.confidence_score.is_none() {
                sym.confidence_score = Some(default_confidence);
            }
        }

        Ok(AdapterOutput {
            symbols,
            source_adapter: ADAPTER_ID.to_string(),
            quality_level: QualityLevel::Semantic,
        })
    }
}

/// Parses the navtree response body into a list of `NavTreeItem`s.
///
/// The navtree response may be a single root item (with `childItems`)
/// or directly a list of items.
fn parse_navtree_body(
    body: &serde_json::Value,
    path: &std::path::Path,
) -> Result<Vec<NavTreeItem>, AdapterError> {
    // tsserver returns navtree as a single root item. The children of
    // the root represent the top-level symbols in the file.
    if let Ok(root) = serde_json::from_value::<NavTreeItem>(body.clone()) {
        return Ok(root.child_items);
    }

    // Fallback: try parsing as a direct array.
    if let Ok(items) = serde_json::from_value::<Vec<NavTreeItem>>(body.clone()) {
        return Ok(items);
    }

    Err(AdapterError::Parse {
        path: path.to_path_buf(),
        reason: "failed to parse navtree response body".to_string(),
    })
}

/// Returns the tsserver script kind based on file extension.
fn script_kind_for_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tsx") => "TSX",
        Some("jsx") => "JSX",
        Some("js") | Some("mjs") | Some("cjs") => "JS",
        _ => "TS",
    }
}

/// Converts a `TsServerError` into an `AdapterError`.
fn ts_error_to_adapter_error(error: TsServerError, path: &std::path::Path) -> AdapterError {
    match error {
        TsServerError::Timeout { operation } => AdapterError::Parse {
            path: path.to_path_buf(),
            reason: format!("tsserver timed out: {operation}"),
        },
        TsServerError::Io { source } => AdapterError::Io {
            path: Some(path.to_path_buf()),
            source,
        },
        other => AdapterError::Parse {
            path: path.to_path_buf(),
            reason: other.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TsServerResponse;
    use std::path::PathBuf;

    /// A mock runtime that returns canned navtree responses for testing
    /// the adapter without a real tsserver process.
    struct MockRuntime {
        navtree_response: Option<serde_json::Value>,
        started: bool,
        request_log: Vec<String>,
    }

    impl MockRuntime {
        fn new(navtree_body: serde_json::Value) -> Self {
            Self {
                navtree_response: Some(navtree_body),
                started: true,
                request_log: Vec::new(),
            }
        }

        fn failing() -> Self {
            Self {
                navtree_response: None,
                started: true,
                request_log: Vec::new(),
            }
        }
    }

    impl SemanticRuntime for MockRuntime {
        fn start(&mut self) -> Result<(), TsServerError> {
            self.started = true;
            Ok(())
        }

        fn stop(&mut self) {
            self.started = false;
        }

        fn restart(&mut self) -> Result<(), TsServerError> {
            self.started = true;
            Ok(())
        }

        fn is_healthy(&mut self) -> bool {
            self.started
        }

        fn send_request(
            &mut self,
            command: &str,
            _arguments: Option<serde_json::Value>,
        ) -> Result<TsServerResponse, TsServerError> {
            self.request_log.push(command.to_string());

            match command {
                "open" | "close" => Ok(TsServerResponse {
                    seq: 0,
                    msg_type: "response".to_string(),
                    command: Some(command.to_string()),
                    request_seq: Some(1),
                    success: Some(true),
                    body: None,
                    message: None,
                }),
                "navtree" => {
                    if let Some(body) = &self.navtree_response {
                        Ok(TsServerResponse {
                            seq: 0,
                            msg_type: "response".to_string(),
                            command: Some("navtree".to_string()),
                            request_seq: Some(2),
                            success: Some(true),
                            body: Some(body.clone()),
                            message: None,
                        })
                    } else {
                        Err(TsServerError::Protocol {
                            reason: "mock: no navtree response configured".to_string(),
                        })
                    }
                }
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

    fn make_context() -> IndexContext {
        IndexContext {
            repo_id: "test-repo".to_string(),
            source_root: PathBuf::from("/tmp/test-repo"),
        }
    }

    fn make_ts_file(content: &str) -> SourceFile {
        SourceFile {
            relative_path: PathBuf::from("src/main.ts"),
            absolute_path: PathBuf::from("/tmp/test-repo/src/main.ts"),
            content: content.as_bytes().to_vec(),
            language: "typescript".to_string(),
        }
    }

    fn navtree_body(child_items: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "text": "<global>",
            "kind": "script",
            "kindModifiers": "",
            "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            "childItems": child_items
        })
    }

    // -- Identity and capabilities --

    #[test]
    fn adapter_id_follows_naming_convention() {
        let rt = MockRuntime::new(navtree_body(serde_json::json!([])));
        let adapter = TypeScriptSemanticAdapter::new(rt);
        assert_eq!(adapter.adapter_id(), "semantic-typescript-v1");
        assert_eq!(adapter.language(), "typescript");
    }

    #[test]
    fn capabilities_are_semantic_level() {
        let rt = MockRuntime::new(navtree_body(serde_json::json!([])));
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let caps = adapter.capabilities();
        assert_eq!(caps.quality_level, QualityLevel::Semantic);
        assert!((caps.default_confidence - 0.9).abs() < f32::EPSILON);
        assert!(caps.supports_type_refs);
        assert!(caps.supports_call_refs);
    }

    // -- Language rejection --

    #[test]
    fn rejects_unsupported_language() {
        let rt = MockRuntime::new(navtree_body(serde_json::json!([])));
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = SourceFile {
            language: "python".to_string(),
            ..make_ts_file("x = 1")
        };
        let err = adapter.index_file(&ctx, &file).expect_err("should reject");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Empty file --

    #[test]
    fn empty_file_produces_no_symbols() {
        let rt = MockRuntime::new(navtree_body(serde_json::json!([])));
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file("");
        let output = adapter.index_file(&ctx, &file).unwrap();
        assert!(output.symbols.is_empty());
        assert_eq!(output.source_adapter, "semantic-typescript-v1");
        assert_eq!(output.quality_level, QualityLevel::Semantic);
    }

    // -- Function extraction --

    #[test]
    fn extracts_function_from_navtree() {
        let body = navtree_body(serde_json::json!([
            {
                "text": "greet",
                "kind": "function",
                "kindModifiers": "export",
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 38}}],
                "childItems": []
            }
        ]));
        let source = "export function greet(name: string) {}";
        let rt = MockRuntime::new(body);
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file(source);
        let output = adapter.index_file(&ctx, &file).unwrap();

        assert_eq!(output.symbols.len(), 1);
        let sym = &output.symbols[0];
        assert_eq!(sym.name, "greet");
        assert_eq!(sym.kind, core_model::SymbolKind::Function);
        assert_eq!(sym.qualified_name, "greet");
        assert_eq!(sym.signature, "export function greet");
        assert!(sym.confidence_score.is_some());
        assert!((sym.confidence_score.unwrap() - 0.9).abs() < f32::EPSILON);
    }

    // -- Class with methods --

    #[test]
    fn extracts_class_with_methods() {
        let body = navtree_body(serde_json::json!([
            {
                "text": "Calculator",
                "kind": "class",
                "kindModifiers": "",
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 4, "offset": 2}}],
                "childItems": [
                    {
                        "text": "add",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 2, "offset": 3}, "end": {"line": 2, "offset": 40}}],
                        "childItems": []
                    },
                    {
                        "text": "subtract",
                        "kind": "method",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 3, "offset": 3}, "end": {"line": 3, "offset": 45}}],
                        "childItems": []
                    }
                ]
            }
        ]));
        let source = "class Calculator {\n  add(a: number, b: number): number {}\n  subtract(a: number, b: number): number {}\n}\n";
        let rt = MockRuntime::new(body);
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file(source);
        let output = adapter.index_file(&ctx, &file).unwrap();

        assert_eq!(output.symbols.len(), 3);
        assert_eq!(output.symbols[0].name, "Calculator");
        assert_eq!(output.symbols[0].kind, core_model::SymbolKind::Class);

        assert_eq!(output.symbols[1].name, "add");
        assert_eq!(output.symbols[1].kind, core_model::SymbolKind::Method);
        assert_eq!(output.symbols[1].qualified_name, "Calculator::add");
        assert_eq!(
            output.symbols[1].parent_qualified_name.as_deref(),
            Some("Calculator")
        );
    }

    // -- Provenance --

    #[test]
    fn output_carries_provenance_fields() {
        let body = navtree_body(serde_json::json!([
            {
                "text": "hello",
                "kind": "function",
                "kindModifiers": "",
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 20}}],
                "childItems": []
            }
        ]));
        let rt = MockRuntime::new(body);
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file("function hello() {}");
        let output = adapter.index_file(&ctx, &file).unwrap();

        assert_eq!(output.source_adapter, "semantic-typescript-v1");
        assert_eq!(output.quality_level, QualityLevel::Semantic);
    }

    // -- Confidence resolution --

    #[test]
    fn all_symbols_have_resolved_confidence() {
        let body = navtree_body(serde_json::json!([
            {
                "text": "a",
                "kind": "function",
                "kindModifiers": "",
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 17}}],
                "childItems": []
            },
            {
                "text": "B",
                "kind": "class",
                "kindModifiers": "",
                "spans": [{"start": {"line": 2, "offset": 1}, "end": {"line": 2, "offset": 12}}],
                "childItems": []
            }
        ]));
        let source = "function a() {}\nclass B {}\n";
        let rt = MockRuntime::new(body);
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file(source);
        let output = adapter.index_file(&ctx, &file).unwrap();

        for sym in &output.symbols {
            let score = sym
                .confidence_score
                .unwrap_or_else(|| panic!("symbol '{}' missing confidence", sym.name));
            assert!(
                (0.0..=1.0).contains(&score),
                "symbol '{}' confidence {score} out of range",
                sym.name
            );
        }
    }

    // -- Error handling --

    #[test]
    fn navtree_failure_returns_parse_error() {
        let rt = MockRuntime::failing();
        let adapter = TypeScriptSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_ts_file("function broken() {}");
        let err = adapter
            .index_file(&ctx, &file)
            .expect_err("should fail on navtree");
        assert!(err.to_string().contains("protocol error") || err.to_string().contains("parse"));
    }

    // -- Script kind detection --

    #[test]
    fn script_kind_for_ts_files() {
        assert_eq!(script_kind_for_path(&PathBuf::from("foo.ts")), "TS");
        assert_eq!(script_kind_for_path(&PathBuf::from("foo.tsx")), "TSX");
        assert_eq!(script_kind_for_path(&PathBuf::from("foo.js")), "JS");
        assert_eq!(script_kind_for_path(&PathBuf::from("foo.jsx")), "JSX");
        assert_eq!(script_kind_for_path(&PathBuf::from("foo.mjs")), "JS");
    }

    // -- Determinism --

    #[test]
    fn extraction_is_deterministic() {
        let body = navtree_body(serde_json::json!([
            {
                "text": "Config",
                "kind": "interface",
                "kindModifiers": "export",
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 3, "offset": 2}}],
                "childItems": [
                    {
                        "text": "name",
                        "kind": "property",
                        "kindModifiers": "",
                        "spans": [{"start": {"line": 2, "offset": 3}, "end": {"line": 2, "offset": 16}}],
                        "childItems": []
                    }
                ]
            },
            {
                "text": "create",
                "kind": "function",
                "kindModifiers": "export",
                "spans": [{"start": {"line": 4, "offset": 1}, "end": {"line": 4, "offset": 40}}],
                "childItems": []
            }
        ]));
        let source =
            "interface Config {\n  name: string;\n}\nexport function create(): Config {}\n";

        let run = |body: &serde_json::Value| {
            let rt = MockRuntime::new(body.clone());
            let adapter = TypeScriptSemanticAdapter::new(rt);
            let ctx = make_context();
            let file = make_ts_file(source);
            adapter.index_file(&ctx, &file).unwrap()
        };

        let out1 = run(&body);
        let out2 = run(&body);

        assert_eq!(out1.symbols.len(), out2.symbols.len());
        for (a, b) in out1.symbols.iter().zip(out2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
            assert_eq!(a.confidence_score, b.confidence_score);
        }
    }
}

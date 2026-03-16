use core_model::QualityLevel;

use std::cell::RefCell;

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, IndexContext, LanguageAdapter, SourceFile,
};
use tracing::warn;

use crate::error::KotlinAnalysisError;
use crate::mapping::{map_kt_navtree_to_symbols, KtNavTreeItem};
use crate::runtime::KotlinRuntime;

const ADAPTER_ID: &str = "semantic-kotlin-v1";
const LANGUAGE: &str = "kotlin";

/// A semantic adapter for Kotlin that uses a JVM analysis bridge to
/// extract symbols with high-confidence, type-aware metadata.
///
/// The adapter communicates with a Kotlin analysis bridge process
/// through the [`KotlinRuntime`] trait, enabling test doubles without
/// real JVM processes.
pub struct KotlinSemanticAdapter<R: KotlinRuntime> {
    runtime: RefCell<R>,
    capabilities: AdapterCapabilities,
}

impl<R: KotlinRuntime> KotlinSemanticAdapter<R> {
    /// Creates a new Kotlin semantic adapter wrapping the given runtime.
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

    /// Sends an `analyze` request to the bridge for the given file and
    /// parses the response into `KtNavTreeItem`s.
    fn request_analysis(&self, file: &SourceFile) -> Result<Vec<KtNavTreeItem>, AdapterError> {
        let content = String::from_utf8_lossy(&file.content).to_string();
        let mut rt = self.runtime.borrow_mut();

        let analyze_args = serde_json::json!({
            "file": file.absolute_path.to_string_lossy(),
            "content": content,
        });

        let response = rt
            .send_request("analyze", Some(analyze_args))
            .map_err(|e| {
                warn!(file = %file.relative_path.display(), error = %e, "kotlin analysis failed");
                kt_error_to_adapter_error(e, &file.relative_path)
            })?;

        let body = response.body.ok_or_else(|| AdapterError::Parse {
            path: file.relative_path.clone(),
            reason: "analyze response has no body".to_string(),
        })?;

        parse_analysis_body(&body, &file.relative_path)
    }
}

impl<R: KotlinRuntime> LanguageAdapter for KotlinSemanticAdapter<R> {
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

        if file.content.is_empty() {
            return Ok(AdapterOutput {
                symbols: vec![],
                source_adapter: ADAPTER_ID.to_string(),
                quality_level: QualityLevel::Semantic,
            });
        }

        let navtree_items = self.request_analysis(file)?;
        let mut symbols = map_kt_navtree_to_symbols(&navtree_items);

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

/// Parses the analysis response body into a list of `KtNavTreeItem`s.
fn parse_analysis_body(
    body: &serde_json::Value,
    path: &std::path::Path,
) -> Result<Vec<KtNavTreeItem>, AdapterError> {
    // Try parsing as a direct array of items.
    if let Ok(items) = serde_json::from_value::<Vec<KtNavTreeItem>>(body.clone()) {
        return Ok(items);
    }

    // Try parsing as a root wrapper with childItems.
    if let Ok(root) = serde_json::from_value::<KtNavTreeItem>(body.clone()) {
        return Ok(root.child_items);
    }

    Err(AdapterError::Parse {
        path: path.to_path_buf(),
        reason: "failed to parse analysis response body".to_string(),
    })
}

/// Converts a `KotlinAnalysisError` into an `AdapterError`.
fn kt_error_to_adapter_error(error: KotlinAnalysisError, path: &std::path::Path) -> AdapterError {
    match error {
        KotlinAnalysisError::Timeout { operation } => AdapterError::Parse {
            path: path.to_path_buf(),
            reason: format!("kotlin analysis bridge timed out: {operation}"),
        },
        KotlinAnalysisError::Io { source } => AdapterError::Io {
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
    use crate::protocol::KotlinResponse;
    use std::path::PathBuf;

    /// A mock runtime that returns canned analysis responses.
    struct MockRuntime {
        analysis_response: Option<serde_json::Value>,
        started: bool,
    }

    impl MockRuntime {
        fn new(body: serde_json::Value) -> Self {
            Self {
                analysis_response: Some(body),
                started: true,
            }
        }

        fn failing() -> Self {
            Self {
                analysis_response: None,
                started: true,
            }
        }
    }

    impl KotlinRuntime for MockRuntime {
        fn start(&mut self) -> Result<(), KotlinAnalysisError> {
            self.started = true;
            Ok(())
        }

        fn stop(&mut self) {
            self.started = false;
        }

        fn restart(&mut self) -> Result<(), KotlinAnalysisError> {
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
        ) -> Result<KotlinResponse, KotlinAnalysisError> {
            match command {
                "analyze" => {
                    if let Some(body) = &self.analysis_response {
                        Ok(KotlinResponse {
                            seq: 0,
                            msg_type: "response".to_string(),
                            command: Some("analyze".to_string()),
                            request_seq: Some(1),
                            success: Some(true),
                            body: Some(body.clone()),
                            message: None,
                        })
                    } else {
                        Err(KotlinAnalysisError::Protocol {
                            reason: "mock: no analysis response configured".to_string(),
                        })
                    }
                }
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

    fn make_context() -> IndexContext {
        IndexContext {
            repo_id: "test-repo".to_string(),
            source_root: PathBuf::from("/tmp/test-repo"),
        }
    }

    fn make_kt_file(content: &str) -> SourceFile {
        SourceFile {
            relative_path: PathBuf::from("src/Main.kt"),
            absolute_path: PathBuf::from("/tmp/test-repo/src/Main.kt"),
            content: content.as_bytes().to_vec(),
            language: "kotlin".to_string(),
        }
    }

    // -- Identity and capabilities --

    #[test]
    fn adapter_id_follows_naming_convention() {
        let rt = MockRuntime::new(serde_json::json!([]));
        let adapter = KotlinSemanticAdapter::new(rt);
        assert_eq!(adapter.adapter_id(), "semantic-kotlin-v1");
        assert_eq!(adapter.language(), "kotlin");
    }

    #[test]
    fn capabilities_are_semantic_level() {
        let rt = MockRuntime::new(serde_json::json!([]));
        let adapter = KotlinSemanticAdapter::new(rt);
        let caps = adapter.capabilities();
        assert_eq!(caps.quality_level, QualityLevel::Semantic);
        assert!((caps.default_confidence - 0.9).abs() < f32::EPSILON);
        assert!(caps.supports_type_refs);
        assert!(caps.supports_call_refs);
    }

    // -- Language rejection --

    #[test]
    fn rejects_unsupported_language() {
        let rt = MockRuntime::new(serde_json::json!([]));
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = SourceFile {
            language: "python".to_string(),
            ..make_kt_file("x = 1")
        };
        let err = adapter.index_file(&ctx, &file).expect_err("should reject");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Empty file --

    #[test]
    fn empty_file_produces_no_symbols() {
        let rt = MockRuntime::new(serde_json::json!([]));
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file("");
        let output = adapter.index_file(&ctx, &file).unwrap();
        assert!(output.symbols.is_empty());
        assert_eq!(output.source_adapter, "semantic-kotlin-v1");
        assert_eq!(output.quality_level, QualityLevel::Semantic);
    }

    // -- Function extraction --

    #[test]
    fn extracts_function_from_analysis() {
        let body = serde_json::json!([
            {
                "name": "greet",
                "kind": "fun",
                "modifiers": "public",
                "signature": "fun greet(name: String): String",
                "startLine": 1,
                "endLine": 3,
                "startByte": 0,
                "byteLengthField": 60,
                "byteLength": 60,
                "childItems": []
            }
        ]);
        let source = "fun greet(name: String): String {\n    return \"Hello, $name\"\n}";
        let rt = MockRuntime::new(body);
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file(source);
        let output = adapter.index_file(&ctx, &file).unwrap();

        assert_eq!(output.symbols.len(), 1);
        let sym = &output.symbols[0];
        assert_eq!(sym.name, "greet");
        assert_eq!(sym.kind, core_model::SymbolKind::Function);
        assert_eq!(sym.qualified_name, "greet");
        assert_eq!(sym.signature, "fun greet(name: String): String");
        assert!((sym.confidence_score.unwrap() - 0.9).abs() < f32::EPSILON);
    }

    // -- Class with methods --

    #[test]
    fn extracts_class_with_methods() {
        let body = serde_json::json!([
            {
                "name": "Calculator",
                "kind": "class",
                "modifiers": "",
                "startLine": 1,
                "endLine": 6,
                "startByte": 0,
                "byteLength": 120,
                "childItems": [
                    {
                        "name": "add",
                        "kind": "fun",
                        "modifiers": "",
                        "startLine": 2,
                        "endLine": 2,
                        "startByte": 20,
                        "byteLength": 40,
                        "childItems": []
                    },
                    {
                        "name": "subtract",
                        "kind": "fun",
                        "modifiers": "",
                        "startLine": 4,
                        "endLine": 4,
                        "startByte": 62,
                        "byteLength": 45,
                        "childItems": []
                    }
                ]
            }
        ]);
        let source = "class Calculator {\n    fun add(a: Int, b: Int): Int = a + b\n\n    fun subtract(a: Int, b: Int): Int = a - b\n}\n";
        let rt = MockRuntime::new(body);
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file(source);
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
        let body = serde_json::json!([
            {
                "name": "hello",
                "kind": "fun",
                "modifiers": "",
                "startLine": 1,
                "endLine": 1,
                "startByte": 0,
                "byteLength": 20,
                "childItems": []
            }
        ]);
        let rt = MockRuntime::new(body);
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file("fun hello() {}");
        let output = adapter.index_file(&ctx, &file).unwrap();

        assert_eq!(output.source_adapter, "semantic-kotlin-v1");
        assert_eq!(output.quality_level, QualityLevel::Semantic);
    }

    // -- Confidence resolution --

    #[test]
    fn all_symbols_have_resolved_confidence() {
        let body = serde_json::json!([
            {
                "name": "a",
                "kind": "fun",
                "modifiers": "",
                "startLine": 1,
                "endLine": 1,
                "startByte": 0,
                "byteLength": 15,
                "childItems": []
            },
            {
                "name": "B",
                "kind": "class",
                "modifiers": "",
                "startLine": 2,
                "endLine": 2,
                "startByte": 16,
                "byteLength": 10,
                "childItems": []
            }
        ]);
        let source = "fun a() {}\nclass B {}";
        let rt = MockRuntime::new(body);
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file(source);
        let output = adapter.index_file(&ctx, &file).unwrap();

        for sym in &output.symbols {
            let score = sym
                .confidence_score
                .unwrap_or_else(|| panic!("symbol '{}' missing confidence", sym.name));
            assert!((0.0..=1.0).contains(&score));
        }
    }

    // -- Error handling --

    #[test]
    fn analysis_failure_returns_parse_error() {
        let rt = MockRuntime::failing();
        let adapter = KotlinSemanticAdapter::new(rt);
        let ctx = make_context();
        let file = make_kt_file("fun broken() {}");
        let err = adapter
            .index_file(&ctx, &file)
            .expect_err("should fail on analysis");
        assert!(err.to_string().contains("protocol error") || err.to_string().contains("parse"));
    }

    // -- Determinism --

    #[test]
    fn extraction_is_deterministic() {
        let body = serde_json::json!([
            {
                "name": "Config",
                "kind": "interface",
                "modifiers": "",
                "startLine": 1,
                "endLine": 3,
                "startByte": 0,
                "byteLength": 50,
                "childItems": []
            },
            {
                "name": "create",
                "kind": "fun",
                "modifiers": "",
                "startLine": 5,
                "endLine": 7,
                "startByte": 52,
                "byteLength": 40,
                "childItems": []
            }
        ]);
        let source = "interface Config {\n    val name: String\n}\n\nfun create(): Config = TODO()";

        let run = |body: &serde_json::Value| {
            let rt = MockRuntime::new(body.clone());
            let adapter = KotlinSemanticAdapter::new(rt);
            let ctx = make_context();
            let file = make_kt_file(source);
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

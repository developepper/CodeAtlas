//! Integration tests for adapter routing policy.
//!
//! These tests exercise the full public API surface of [`DefaultRouter`]
//! combined with [`default_policy`], verifying end-to-end routing behavior
//! with mock adapters of varying quality levels.

use adapter_api::router::{default_policy, DefaultRouter};
use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, AdapterPolicy, AdapterRouter,
    ExtractedSymbol, IndexContext, LanguageAdapter, SourceFile, SourceSpan,
};
use core_model::{QualityLevel, SymbolKind};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Mock adapters (external crate perspective — only public API)
// ---------------------------------------------------------------------------

struct MockSyntaxRust;

impl LanguageAdapter for MockSyntaxRust {
    fn adapter_id(&self) -> &str {
        "syntax-rust"
    }
    fn language(&self) -> &str {
        "rust"
    }
    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: true,
            supports_doc_extraction: true,
        };
        &CAPS
    }
    fn index_file(
        &self,
        _ctx: &IndexContext,
        _file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        Ok(stub_output("syntax-rust", QualityLevel::Syntax))
    }
}

struct MockSemanticRust;

impl LanguageAdapter for MockSemanticRust {
    fn adapter_id(&self) -> &str {
        "semantic-rust"
    }
    fn language(&self) -> &str {
        "rust"
    }
    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Semantic,
            default_confidence: 0.9,
            supports_type_refs: true,
            supports_call_refs: true,
            supports_container_refs: true,
            supports_doc_extraction: true,
        };
        &CAPS
    }
    fn index_file(
        &self,
        _ctx: &IndexContext,
        _file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        Ok(stub_output("semantic-rust", QualityLevel::Semantic))
    }
}

struct MockSyntaxTypescript;

impl LanguageAdapter for MockSyntaxTypescript {
    fn adapter_id(&self) -> &str {
        "syntax-typescript"
    }
    fn language(&self) -> &str {
        "typescript"
    }
    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: true,
            supports_doc_extraction: true,
        };
        &CAPS
    }
    fn index_file(
        &self,
        _ctx: &IndexContext,
        _file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        Ok(stub_output("syntax-typescript", QualityLevel::Syntax))
    }
}

struct MockSemanticTypescript;

impl LanguageAdapter for MockSemanticTypescript {
    fn adapter_id(&self) -> &str {
        "semantic-typescript"
    }
    fn language(&self) -> &str {
        "typescript"
    }
    fn capabilities(&self) -> &AdapterCapabilities {
        const CAPS: AdapterCapabilities = AdapterCapabilities {
            quality_level: QualityLevel::Semantic,
            default_confidence: 0.9,
            supports_type_refs: true,
            supports_call_refs: true,
            supports_container_refs: true,
            supports_doc_extraction: true,
        };
        &CAPS
    }
    fn index_file(
        &self,
        _ctx: &IndexContext,
        _file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        Ok(stub_output("semantic-typescript", QualityLevel::Semantic))
    }
}

fn stub_output(adapter_id: &str, quality: QualityLevel) -> AdapterOutput {
    AdapterOutput {
        symbols: vec![ExtractedSymbol {
            name: "stub".to_string(),
            qualified_name: "stub".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 1,
            },
            signature: "fn stub()".to_string(),
            confidence_score: None,
            docstring: None,
            parent_qualified_name: None,
        }],
        source_adapter: adapter_id.to_string(),
        quality_level: quality,
    }
}

fn mock_context() -> IndexContext {
    IndexContext {
        repo_id: "test-repo".to_string(),
        source_root: PathBuf::from("/tmp/test-repo"),
    }
}

fn mock_file(language: &str) -> SourceFile {
    SourceFile {
        relative_path: PathBuf::from("src/main.rs"),
        absolute_path: PathBuf::from("/tmp/test-repo/src/main.rs"),
        content: b"fn main() {}\n".to_vec(),
        language: language.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Integration: default_policy + router selection
// ---------------------------------------------------------------------------

#[test]
fn default_policy_routes_rust_to_syntax_only() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxRust));
    router.register(Box::new(MockSemanticRust));

    let policy = default_policy("rust");
    assert_eq!(policy, AdapterPolicy::SyntaxOnly);

    let selected = router.select("rust", policy);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].adapter_id(), "syntax-rust");
}

#[test]
fn default_policy_routes_typescript_semantic_preferred() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxTypescript));
    router.register(Box::new(MockSemanticTypescript));

    let policy = default_policy("typescript");
    assert_eq!(policy, AdapterPolicy::SemanticPreferred);

    let selected = router.select("typescript", policy);
    assert_eq!(selected.len(), 2);
    // Semantic first.
    assert_eq!(selected[0].adapter_id(), "semantic-typescript");
    assert_eq!(selected[1].adapter_id(), "syntax-typescript");
}

#[test]
fn semantic_preferred_falls_back_when_no_semantic_available() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxTypescript));

    let policy = default_policy("typescript");
    assert_eq!(policy, AdapterPolicy::SemanticPreferred);

    let selected = router.select("typescript", policy);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].adapter_id(), "syntax-typescript");
}

#[test]
fn semantic_required_returns_empty_when_only_syntax_available() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxRust));

    let selected = router.select("rust", AdapterPolicy::SemanticRequired);
    assert!(selected.is_empty(), "should return empty, not panic");
}

// ---------------------------------------------------------------------------
// Integration: selected adapter produces valid output
// ---------------------------------------------------------------------------

#[test]
fn selected_adapter_produces_output_with_correct_provenance() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxRust));
    router.register(Box::new(MockSemanticRust));

    // SemanticPreferred: first result should be semantic.
    let selected = router.select("rust", AdapterPolicy::SemanticPreferred);
    let adapter = selected[0];
    assert_eq!(adapter.capabilities().quality_level, QualityLevel::Semantic);

    let ctx = mock_context();
    let file = mock_file("rust");
    let output = adapter.index_file(&ctx, &file).expect("index_file");

    assert_eq!(output.source_adapter, "semantic-rust");
    assert_eq!(output.quality_level, QualityLevel::Semantic);
    assert!(!output.symbols.is_empty());
}

#[test]
fn multi_language_routing_is_independent() {
    let mut router = DefaultRouter::new();
    router.register(Box::new(MockSyntaxRust));
    router.register(Box::new(MockSemanticRust));
    router.register(Box::new(MockSyntaxTypescript));
    router.register(Box::new(MockSemanticTypescript));

    // Rust with SyntaxOnly: only syntax.
    let rust = router.select("rust", AdapterPolicy::SyntaxOnly);
    assert_eq!(rust.len(), 1);
    assert_eq!(rust[0].adapter_id(), "syntax-rust");

    // TypeScript with SemanticPreferred: both, semantic first.
    let ts = router.select("typescript", AdapterPolicy::SemanticPreferred);
    assert_eq!(ts.len(), 2);
    assert_eq!(ts[0].adapter_id(), "semantic-typescript");

    // Unregistered language: empty.
    let py = router.select("python", AdapterPolicy::SemanticPreferred);
    assert!(py.is_empty());
}

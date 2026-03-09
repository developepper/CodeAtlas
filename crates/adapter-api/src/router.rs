//! Concrete adapter routing implementation.
//!
//! Provides [`DefaultRouter`], the production implementation of
//! [`AdapterRouter`] that selects adapters based on language and
//! [`AdapterPolicy`], and [`default_policy`] which maps languages to
//! their default routing policy per spec §5.2.

use core_model::QualityLevel;

use crate::{AdapterPolicy, AdapterRouter, LanguageAdapter};

// ---------------------------------------------------------------------------
// Default policy mapping (spec §5.2)
// ---------------------------------------------------------------------------

/// Returns the default [`AdapterPolicy`] for a language.
///
/// Per spec §5.2:
/// - Kotlin, Java, TypeScript, PHP → `SemanticPreferred`
/// - All others → `SyntaxOnly` (until a semantic adapter is implemented)
#[must_use]
pub fn default_policy(language: &str) -> AdapterPolicy {
    match language {
        "kotlin" | "java" | "typescript" | "php" => AdapterPolicy::SemanticPreferred,
        _ => AdapterPolicy::SyntaxOnly,
    }
}

// ---------------------------------------------------------------------------
// DefaultRouter
// ---------------------------------------------------------------------------

/// Production implementation of [`AdapterRouter`].
///
/// Adapters are registered via [`register`](DefaultRouter::register) and
/// selected via the [`AdapterRouter::select`] trait method. Selection filters
/// by language and policy, returning adapters in quality-priority order
/// (semantic before syntax).
pub struct DefaultRouter {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl DefaultRouter {
    /// Creates an empty router with no registered adapters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Registers an adapter. Adapters may be registered in any order;
    /// [`select`](AdapterRouter::select) sorts results by quality.
    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) {
        self.adapters.push(adapter);
    }

    /// Returns all registered adapter IDs (for diagnostics / logging).
    #[must_use]
    pub fn registered_adapter_ids(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.adapter_id()).collect()
    }

    /// Returns the number of registered adapters.
    #[must_use]
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }
}

impl Default for DefaultRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterRouter for DefaultRouter {
    /// Select adapters for the given language and policy.
    ///
    /// Returns adapters in priority order (highest quality first):
    /// semantic adapters before syntax adapters. Within the same quality
    /// level, adapters are returned in registration order.
    ///
    /// Returns an empty vec when no adapter matches.
    fn select(&self, language: &str, policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        let mut matched: Vec<&dyn LanguageAdapter> = self
            .adapters
            .iter()
            .filter(|a| a.language() == language)
            .filter(|a| a.capabilities().satisfies(policy))
            .map(|a| a.as_ref())
            .collect();

        // Sort by quality: semantic first (deterministic — stable sort
        // preserves registration order within the same quality level).
        matched.sort_by(|a, b| {
            quality_rank(b.capabilities().quality_level)
                .cmp(&quality_rank(a.capabilities().quality_level))
        });

        matched
    }
}

/// Maps quality level to a sort rank (higher = better).
fn quality_rank(level: QualityLevel) -> u8 {
    match level {
        QualityLevel::Syntax => 0,
        QualityLevel::Semantic => 1,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AdapterCapabilities, AdapterError, AdapterOutput, IndexContext, SourceFile, SourceSpan,
    };
    use core_model::SymbolKind;

    // -- Mock adapters for routing tests --

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
            Ok(stub_output("syntax-rust"))
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
            Ok(stub_output("semantic-rust"))
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
            Ok(stub_output("syntax-typescript"))
        }
    }

    fn stub_output(adapter_id: &str) -> AdapterOutput {
        use crate::ExtractedSymbol;
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
            quality_level: QualityLevel::Syntax,
        }
    }

    // -- DefaultRouter construction --

    #[test]
    fn new_router_is_empty() {
        let router = DefaultRouter::new();
        assert_eq!(router.adapter_count(), 0);
        assert!(router.registered_adapter_ids().is_empty());
    }

    #[test]
    fn default_trait_creates_empty_router() {
        let router = DefaultRouter::default();
        assert_eq!(router.adapter_count(), 0);
    }

    #[test]
    fn register_adds_adapters() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));
        router.register(Box::new(MockSemanticRust));
        assert_eq!(router.adapter_count(), 2);
        assert_eq!(
            router.registered_adapter_ids(),
            vec!["syntax-rust", "semantic-rust"]
        );
    }

    // -- Routing matrix: SyntaxOnly --

    #[test]
    fn syntax_only_returns_syntax_adapter() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));
        router.register(Box::new(MockSemanticRust));

        let selected = router.select("rust", AdapterPolicy::SyntaxOnly);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].adapter_id(), "syntax-rust");
    }

    #[test]
    fn syntax_only_excludes_semantic_adapter() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSemanticRust));

        let selected = router.select("rust", AdapterPolicy::SyntaxOnly);
        assert!(selected.is_empty());
    }

    // -- Routing matrix: SemanticRequired --

    #[test]
    fn semantic_required_returns_semantic_adapter() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));
        router.register(Box::new(MockSemanticRust));

        let selected = router.select("rust", AdapterPolicy::SemanticRequired);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].adapter_id(), "semantic-rust");
    }

    #[test]
    fn semantic_required_returns_empty_when_no_semantic() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));

        let selected = router.select("rust", AdapterPolicy::SemanticRequired);
        assert!(selected.is_empty());
    }

    // -- Routing matrix: SemanticPreferred --

    #[test]
    fn semantic_preferred_returns_both_semantic_first() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));
        router.register(Box::new(MockSemanticRust));

        let selected = router.select("rust", AdapterPolicy::SemanticPreferred);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].adapter_id(), "semantic-rust");
        assert_eq!(selected[1].adapter_id(), "syntax-rust");
    }

    #[test]
    fn semantic_preferred_falls_back_to_syntax() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));

        let selected = router.select("rust", AdapterPolicy::SemanticPreferred);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].adapter_id(), "syntax-rust");
    }

    // -- Language filtering --

    #[test]
    fn select_filters_by_language() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));
        router.register(Box::new(MockSyntaxTypescript));

        let rust = router.select("rust", AdapterPolicy::SyntaxOnly);
        assert_eq!(rust.len(), 1);
        assert_eq!(rust[0].adapter_id(), "syntax-rust");

        let ts = router.select("typescript", AdapterPolicy::SyntaxOnly);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].adapter_id(), "syntax-typescript");
    }

    #[test]
    fn select_returns_empty_for_unknown_language() {
        let mut router = DefaultRouter::new();
        router.register(Box::new(MockSyntaxRust));

        let selected = router.select("python", AdapterPolicy::SyntaxOnly);
        assert!(selected.is_empty());
    }

    #[test]
    fn select_returns_empty_for_empty_router() {
        let router = DefaultRouter::new();
        let selected = router.select("rust", AdapterPolicy::SemanticPreferred);
        assert!(selected.is_empty());
    }

    // -- Deterministic ordering --

    #[test]
    fn select_preserves_registration_order_within_same_quality() {
        // Register two syntax adapters for the same language (unusual but valid).
        struct SyntaxA;
        struct SyntaxB;

        impl LanguageAdapter for SyntaxA {
            fn adapter_id(&self) -> &str {
                "syntax-a"
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
                Ok(stub_output("syntax-a"))
            }
        }

        impl LanguageAdapter for SyntaxB {
            fn adapter_id(&self) -> &str {
                "syntax-b"
            }
            fn language(&self) -> &str {
                "rust"
            }
            fn capabilities(&self) -> &AdapterCapabilities {
                const CAPS: AdapterCapabilities = AdapterCapabilities {
                    quality_level: QualityLevel::Syntax,
                    default_confidence: 0.8,
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
                Ok(stub_output("syntax-b"))
            }
        }

        let mut router = DefaultRouter::new();
        router.register(Box::new(SyntaxA));
        router.register(Box::new(SyntaxB));

        let selected = router.select("rust", AdapterPolicy::SyntaxOnly);
        assert_eq!(selected.len(), 2);
        // Stable sort preserves registration order within same quality level.
        assert_eq!(selected[0].adapter_id(), "syntax-a");
        assert_eq!(selected[1].adapter_id(), "syntax-b");
    }

    // -- Default policy mapping (spec §5.2) --

    #[test]
    fn default_policy_semantic_preferred_languages() {
        assert_eq!(default_policy("kotlin"), AdapterPolicy::SemanticPreferred);
        assert_eq!(default_policy("java"), AdapterPolicy::SemanticPreferred);
        assert_eq!(
            default_policy("typescript"),
            AdapterPolicy::SemanticPreferred
        );
        assert_eq!(default_policy("php"), AdapterPolicy::SemanticPreferred);
    }

    #[test]
    fn default_policy_syntax_only_for_others() {
        assert_eq!(default_policy("rust"), AdapterPolicy::SyntaxOnly);
        assert_eq!(default_policy("python"), AdapterPolicy::SyntaxOnly);
        assert_eq!(default_policy("go"), AdapterPolicy::SyntaxOnly);
        assert_eq!(default_policy("c"), AdapterPolicy::SyntaxOnly);
        assert_eq!(default_policy("unknown"), AdapterPolicy::SyntaxOnly);
    }
}

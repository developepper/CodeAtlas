// adapter-api intentionally uses QualityLevel until it is retired in Ticket 3.
#![allow(deprecated)]

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use core_model::{QualityLevel, SymbolKind, Validate, ValidationError, ValidationResult};

pub mod router;

#[cfg(feature = "test-harness")]
pub mod contract;

#[cfg(feature = "test-harness")]
pub mod regression;

// ---------------------------------------------------------------------------
// Adapter selection policy
// ---------------------------------------------------------------------------

/// Per-language routing policy that controls which adapter type is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterPolicy {
    /// Fail if no semantic adapter is available.
    SemanticRequired,
    /// Use semantic adapter when available, fall back to syntax.
    SemanticPreferred,
    /// Use syntax adapter only.
    SyntaxOnly,
}

impl AdapterPolicy {
    /// Returns whether this policy accepts a syntax-only result.
    #[must_use]
    pub fn accepts_syntax(&self) -> bool {
        matches!(self, Self::SemanticPreferred | Self::SyntaxOnly)
    }

    /// Returns whether this policy requires a semantic adapter.
    #[must_use]
    pub fn requires_semantic(&self) -> bool {
        matches!(self, Self::SemanticRequired)
    }
}

// ---------------------------------------------------------------------------
// Adapter capabilities
// ---------------------------------------------------------------------------

/// Declares what an adapter can produce, including quality provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterCapabilities {
    /// The quality level this adapter produces (semantic or syntax).
    pub quality_level: QualityLevel,
    /// Baseline confidence score for symbols extracted by this adapter.
    /// Individual symbols may override with per-symbol scores.
    pub default_confidence: f32,
    /// Whether this adapter can resolve type references.
    pub supports_type_refs: bool,
    /// Whether this adapter can resolve call-site references.
    pub supports_call_refs: bool,
    /// Whether this adapter can determine container/parent relationships.
    pub supports_container_refs: bool,
    /// Whether this adapter can extract documentation strings.
    pub supports_doc_extraction: bool,
}

impl AdapterCapabilities {
    /// Creates capabilities for a syntax-level adapter with typical defaults.
    #[must_use]
    pub fn syntax_baseline() -> Self {
        Self {
            quality_level: QualityLevel::Syntax,
            default_confidence: 0.7,
            supports_type_refs: false,
            supports_call_refs: false,
            supports_container_refs: true,
            supports_doc_extraction: true,
        }
    }

    /// Creates capabilities for a semantic-level adapter with typical defaults.
    #[must_use]
    pub fn semantic_baseline() -> Self {
        Self {
            quality_level: QualityLevel::Semantic,
            default_confidence: 0.9,
            supports_type_refs: true,
            supports_call_refs: true,
            supports_container_refs: true,
            supports_doc_extraction: true,
        }
    }

    /// Returns `true` if this adapter satisfies the given policy.
    ///
    /// - `SemanticRequired`: only semantic adapters satisfy.
    /// - `SemanticPreferred`: any adapter satisfies (semantic preferred, syntax accepted).
    /// - `SyntaxOnly`: only syntax adapters satisfy (per spec §5.2: "syntax adapter only").
    #[must_use]
    pub fn satisfies(&self, policy: AdapterPolicy) -> bool {
        match policy {
            AdapterPolicy::SemanticRequired => self.quality_level == QualityLevel::Semantic,
            AdapterPolicy::SemanticPreferred => true,
            AdapterPolicy::SyntaxOnly => self.quality_level == QualityLevel::Syntax,
        }
    }
}

impl Validate for AdapterCapabilities {
    fn validate(&self) -> ValidationResult {
        validate_confidence(self.default_confidence, "default_confidence")
    }
}

// ---------------------------------------------------------------------------
// Source span (re-exported from core-model)
// ---------------------------------------------------------------------------

pub use core_model::SourceSpan;

// ---------------------------------------------------------------------------
// Extracted symbol (adapter output)
// ---------------------------------------------------------------------------

/// A symbol extracted by an adapter, before pipeline enrichment.
///
/// This is the adapter-local output type. The indexer pipeline is responsible
/// for combining it with context (repo ID, file hash, timestamps) to produce
/// a full [`core_model::SymbolRecord`].
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    /// Per-symbol confidence override. When `None`, the adapter's
    /// `default_confidence` from [`AdapterCapabilities`] is used.
    pub confidence_score: Option<f32>,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
}

impl Validate for ExtractedSymbol {
    fn validate(&self) -> ValidationResult {
        if self.name.trim().is_empty() {
            return Err(ValidationError::MissingField { field: "name" });
        }
        if self.qualified_name.trim().is_empty() {
            return Err(ValidationError::MissingField {
                field: "qualified_name",
            });
        }
        self.span.validate()?;
        if let Some(score) = self.confidence_score {
            validate_confidence(score, "confidence_score")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Adapter output
// ---------------------------------------------------------------------------

/// The result of processing a single source file through an adapter.
///
/// Self-describing: carries provenance metadata so downstream consumers
/// do not need to query the adapter separately.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterOutput {
    /// Symbols extracted from the file.
    pub symbols: Vec<ExtractedSymbol>,
    /// Stable identifier of the adapter that produced this output.
    pub source_adapter: String,
    /// Quality level of the adapter that produced this output.
    pub quality_level: QualityLevel,
}

// ---------------------------------------------------------------------------
// Index context and source file
// ---------------------------------------------------------------------------

/// Context provided by the indexer pipeline to each adapter invocation.
#[derive(Debug, Clone)]
pub struct IndexContext {
    /// Unique identifier for the repository being indexed.
    pub repo_id: String,
    /// Absolute path to the repository root.
    pub source_root: PathBuf,
}

/// A source file presented to an adapter for symbol extraction.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path relative to the repository root.
    pub relative_path: PathBuf,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Raw file content (bounded by discovery-stage size caps).
    pub content: Vec<u8>,
    /// Detected language identifier (e.g. "rust", "typescript").
    pub language: String,
}

// ---------------------------------------------------------------------------
// Adapter error
// ---------------------------------------------------------------------------

/// Errors that an adapter may produce during file indexing.
#[derive(Debug)]
pub enum AdapterError {
    /// The adapter could not parse the file.
    Parse { path: PathBuf, reason: String },
    /// An I/O error occurred while the adapter was processing.
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// The adapter does not support the requested language.
    Unsupported { language: String },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { path, reason } => {
                write!(f, "parse error at '{}': {reason}", path.display())
            }
            Self::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "I/O error at '{}': {source}", path.display())
                } else {
                    write!(f, "I/O error: {source}")
                }
            }
            Self::Unsupported { language } => {
                write!(f, "unsupported language: {language}")
            }
        }
    }
}

impl Error for AdapterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Core traits
// ---------------------------------------------------------------------------

/// Contract for a language-specific symbol extractor.
///
/// Each adapter handles one language and produces [`ExtractedSymbol`]s from
/// source files. The indexer pipeline selects adapters via [`AdapterRouter`]
/// based on language and [`AdapterPolicy`].
pub trait LanguageAdapter {
    /// Stable identifier for this adapter (e.g. `"syntax-treesitter-rust"`).
    fn adapter_id(&self) -> &str;

    /// Language this adapter handles (e.g. `"rust"`).
    fn language(&self) -> &str;

    /// Declares the capabilities and quality level of this adapter.
    fn capabilities(&self) -> &AdapterCapabilities;

    /// Extract symbols from a single source file.
    fn index_file(
        &self,
        ctx: &IndexContext,
        file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError>;
}

/// Contract for selecting adapters based on language and policy.
///
/// The router is responsible for returning adapters in priority order:
/// highest-quality first. The caller may use the first adapter that
/// succeeds or merge results from multiple adapters.
pub trait AdapterRouter {
    /// Select adapters for the given language and policy.
    ///
    /// Returns an empty vec when no adapter is registered for the language,
    /// or when no registered adapter satisfies the policy.
    fn select(&self, language: &str, policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter>;
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_confidence(value: f32, field: &'static str) -> ValidationResult {
    if !value.is_finite() {
        return Err(ValidationError::InvalidField {
            field,
            reason: "must be finite",
        });
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(ValidationError::InvalidField {
            field,
            reason: "must be within 0.0..=1.0",
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::QualityLevel;

    // -- Mock adapter for contract tests --

    struct MockSyntaxAdapter;

    impl LanguageAdapter for MockSyntaxAdapter {
        fn adapter_id(&self) -> &str {
            "mock-syntax-rust"
        }

        fn language(&self) -> &str {
            "rust"
        }

        fn capabilities(&self) -> &AdapterCapabilities {
            // Return a reference to a static for the test; real impls may
            // store this in a field.
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
            file: &SourceFile,
        ) -> Result<AdapterOutput, AdapterError> {
            if file.language != "rust" {
                return Err(AdapterError::Unsupported {
                    language: file.language.clone(),
                });
            }
            Ok(AdapterOutput {
                symbols: vec![ExtractedSymbol {
                    name: "main".to_string(),
                    qualified_name: "main".to_string(),
                    kind: SymbolKind::Function,
                    span: SourceSpan {
                        start_line: 1,
                        end_line: 1,
                        start_byte: 0,
                        byte_length: 14,
                    },
                    signature: "fn main()".to_string(),
                    confidence_score: None,
                    docstring: None,
                    parent_qualified_name: None,
                }],
                source_adapter: "mock-syntax-rust".to_string(),
                quality_level: QualityLevel::Syntax,
            })
        }
    }

    fn mock_context() -> IndexContext {
        IndexContext {
            repo_id: "test-repo".to_string(),
            source_root: PathBuf::from("/tmp/test-repo"),
        }
    }

    fn mock_rust_file() -> SourceFile {
        SourceFile {
            relative_path: PathBuf::from("src/main.rs"),
            absolute_path: PathBuf::from("/tmp/test-repo/src/main.rs"),
            content: b"fn main() {}\n".to_vec(),
            language: "rust".to_string(),
        }
    }

    // -- Policy tests --

    #[test]
    fn policy_semantic_required_rejects_syntax() {
        assert!(!AdapterPolicy::SemanticRequired.accepts_syntax());
        assert!(AdapterPolicy::SemanticRequired.requires_semantic());
    }

    #[test]
    fn policy_semantic_preferred_accepts_both() {
        assert!(AdapterPolicy::SemanticPreferred.accepts_syntax());
        assert!(!AdapterPolicy::SemanticPreferred.requires_semantic());
    }

    #[test]
    fn policy_syntax_only_accepts_syntax() {
        assert!(AdapterPolicy::SyntaxOnly.accepts_syntax());
        assert!(!AdapterPolicy::SyntaxOnly.requires_semantic());
    }

    // -- Capabilities tests --

    #[test]
    fn syntax_baseline_has_expected_defaults() {
        let caps = AdapterCapabilities::syntax_baseline();
        assert_eq!(caps.quality_level, QualityLevel::Syntax);
        assert!((caps.default_confidence - 0.7).abs() < f32::EPSILON);
        assert!(!caps.supports_type_refs);
        assert!(!caps.supports_call_refs);
        assert!(caps.supports_container_refs);
        assert!(caps.supports_doc_extraction);
    }

    #[test]
    fn semantic_baseline_has_expected_defaults() {
        let caps = AdapterCapabilities::semantic_baseline();
        assert_eq!(caps.quality_level, QualityLevel::Semantic);
        assert!((caps.default_confidence - 0.9).abs() < f32::EPSILON);
        assert!(caps.supports_type_refs);
        assert!(caps.supports_call_refs);
        assert!(caps.supports_container_refs);
        assert!(caps.supports_doc_extraction);
    }

    #[test]
    fn capabilities_satisfy_matching_policies() {
        let syntax = AdapterCapabilities::syntax_baseline();
        let semantic = AdapterCapabilities::semantic_baseline();

        // Syntax adapter satisfies syntax_only and semantic_preferred, not semantic_required.
        assert!(syntax.satisfies(AdapterPolicy::SyntaxOnly));
        assert!(syntax.satisfies(AdapterPolicy::SemanticPreferred));
        assert!(!syntax.satisfies(AdapterPolicy::SemanticRequired));

        // Semantic adapter satisfies semantic policies but NOT syntax_only.
        assert!(!semantic.satisfies(AdapterPolicy::SyntaxOnly));
        assert!(semantic.satisfies(AdapterPolicy::SemanticPreferred));
        assert!(semantic.satisfies(AdapterPolicy::SemanticRequired));
    }

    // -- Mock adapter contract tests --

    #[test]
    fn mock_adapter_identity_is_stable() {
        let adapter = MockSyntaxAdapter;
        assert_eq!(adapter.adapter_id(), "mock-syntax-rust");
        assert_eq!(adapter.language(), "rust");
    }

    #[test]
    fn mock_adapter_capabilities_include_quality_provenance() {
        let adapter = MockSyntaxAdapter;
        let caps = adapter.capabilities();
        assert_eq!(caps.quality_level, QualityLevel::Syntax);
        assert!(caps.default_confidence > 0.0);
        assert!(caps.default_confidence <= 1.0);
    }

    #[test]
    fn mock_adapter_extracts_symbols_from_supported_language() {
        let adapter = MockSyntaxAdapter;
        let ctx = mock_context();
        let file = mock_rust_file();

        let output = adapter.index_file(&ctx, &file).expect("index file");
        assert_eq!(output.symbols.len(), 1);

        let sym = &output.symbols[0];
        assert_eq!(sym.name, "main");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.span.start_line, 1);
        assert!(sym.span.byte_length > 0);
    }

    #[test]
    fn mock_adapter_rejects_unsupported_language() {
        let adapter = MockSyntaxAdapter;
        let ctx = mock_context();
        let file = SourceFile {
            language: "python".to_string(),
            ..mock_rust_file()
        };

        let err = adapter.index_file(&ctx, &file).expect_err("should fail");
        assert!(err.to_string().contains("unsupported language"));
    }

    #[test]
    fn adapter_error_display_covers_all_variants() {
        let parse_err = AdapterError::Parse {
            path: PathBuf::from("src/main.rs"),
            reason: "unexpected token".to_string(),
        };
        assert!(parse_err.to_string().contains("parse error"));
        assert!(parse_err.to_string().contains("unexpected token"));

        let io_err = AdapterError::Io {
            path: Some(PathBuf::from("src/main.rs")),
            source: std::io::Error::other("disk full"),
        };
        assert!(io_err.to_string().contains("I/O error"));

        let io_err_no_path = AdapterError::Io {
            path: None,
            source: std::io::Error::other("disk full"),
        };
        assert!(io_err_no_path.to_string().contains("I/O error"));

        let unsupported = AdapterError::Unsupported {
            language: "brainfuck".to_string(),
        };
        assert!(unsupported.to_string().contains("unsupported language"));
    }

    // -- Validation tests --

    #[test]
    fn capabilities_validate_rejects_out_of_range_confidence() {
        let mut caps = AdapterCapabilities::syntax_baseline();
        caps.default_confidence = 1.5;
        let err = caps.validate().expect_err("should reject >1.0");
        assert!(err.to_string().contains("default_confidence"));

        caps.default_confidence = -0.1;
        let err = caps.validate().expect_err("should reject <0.0");
        assert!(err.to_string().contains("default_confidence"));

        caps.default_confidence = f32::NAN;
        let err = caps.validate().expect_err("should reject NaN");
        assert!(err.to_string().contains("must be finite"));
    }

    #[test]
    fn capabilities_validate_accepts_valid_range() {
        let caps = AdapterCapabilities::syntax_baseline();
        caps.validate().expect("baseline should be valid");

        let mut edge = AdapterCapabilities::syntax_baseline();
        edge.default_confidence = 0.0;
        edge.validate().expect("0.0 should be valid");
        edge.default_confidence = 1.0;
        edge.validate().expect("1.0 should be valid");
    }

    #[test]
    fn source_span_validate_rejects_zero_start_line() {
        let span = SourceSpan {
            start_line: 0,
            end_line: 1,
            start_byte: 0,
            byte_length: 10,
        };
        let err = span.validate().expect_err("should reject zero start_line");
        assert!(err.to_string().contains("start_line"));
    }

    #[test]
    fn source_span_validate_rejects_end_before_start() {
        let span = SourceSpan {
            start_line: 5,
            end_line: 3,
            start_byte: 0,
            byte_length: 10,
        };
        let err = span.validate().expect_err("should reject end < start");
        assert!(err.to_string().contains("end_line"));
    }

    #[test]
    fn source_span_validate_rejects_zero_byte_length() {
        let span = SourceSpan {
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            byte_length: 0,
        };
        let err = span.validate().expect_err("should reject zero byte_length");
        assert!(err.to_string().contains("byte_length"));
    }

    #[test]
    fn source_span_validate_accepts_valid_span() {
        let span = SourceSpan {
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            byte_length: 14,
        };
        span.validate().expect("valid span");

        // Single-line span where start == end is valid.
        let single = SourceSpan {
            start_line: 10,
            end_line: 10,
            start_byte: 100,
            byte_length: 1,
        };
        single.validate().expect("single-line span");
    }

    #[test]
    fn extracted_symbol_validate_rejects_empty_name() {
        let sym = ExtractedSymbol {
            name: "  ".to_string(),
            qualified_name: "mod::foo".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: "fn foo()".to_string(),
            confidence_score: None,
            docstring: None,
            parent_qualified_name: None,
        };
        let err = sym.validate().expect_err("should reject blank name");
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn extracted_symbol_validate_rejects_invalid_confidence() {
        let sym = ExtractedSymbol {
            name: "foo".to_string(),
            qualified_name: "mod::foo".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: "fn foo()".to_string(),
            confidence_score: Some(2.0),
            docstring: None,
            parent_qualified_name: None,
        };
        let err = sym.validate().expect_err("should reject >1.0");
        assert!(err.to_string().contains("confidence_score"));
    }

    #[test]
    fn extracted_symbol_validate_propagates_span_errors() {
        let sym = ExtractedSymbol {
            name: "foo".to_string(),
            qualified_name: "mod::foo".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 0,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: "fn foo()".to_string(),
            confidence_score: None,
            docstring: None,
            parent_qualified_name: None,
        };
        let err = sym.validate().expect_err("should propagate span error");
        assert!(err.to_string().contains("start_line"));
    }

    #[test]
    fn extracted_symbol_confidence_override() {
        let sym = ExtractedSymbol {
            name: "foo".to_string(),
            qualified_name: "mod::foo".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 10,
                end_line: 20,
                start_byte: 100,
                byte_length: 200,
            },
            signature: "fn foo()".to_string(),
            confidence_score: Some(0.95),
            docstring: Some("Does foo things.".to_string()),
            parent_qualified_name: Some("mod".to_string()),
        };
        assert_eq!(sym.confidence_score, Some(0.95));
        assert!(sym.docstring.is_some());
        assert!(sym.parent_qualified_name.is_some());
    }
}

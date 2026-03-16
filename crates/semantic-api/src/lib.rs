//! Semantic backend API for CodeAtlas.
//!
//! Defines the [`SemanticBackend`] trait and associated types for semantic
//! enrichment backends. Semantic backends receive a syntax baseline and
//! produce higher-fidelity symbols with type/call references.

use std::path::PathBuf;

use core_model::{BackendId, SourceSpan, SymbolKind};
use syntax_platform::{PreparedFile, SyntaxMergeBaseline};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Describes the enrichment features a semantic backend provides.
#[derive(Debug, Clone)]
pub struct SemanticCapability {
    /// Whether this backend can resolve type references.
    pub supports_type_refs: bool,
    /// Whether this backend can resolve call-site references.
    pub supports_call_refs: bool,
    /// Default confidence score for symbols produced by this backend.
    pub default_confidence: f32,
}

/// A symbol produced by semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub confidence_score: Option<f32>,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
    /// Type references resolved by semantic analysis.
    pub type_refs: Vec<String>,
    /// Call-site references resolved by semantic analysis.
    pub call_refs: Vec<String>,
}

/// Result of semantic enrichment for a single file.
#[derive(Debug, Clone)]
pub struct SemanticExtraction {
    pub language: String,
    pub symbols: Vec<SemanticSymbol>,
    pub backend_id: BackendId,
    pub default_confidence: f32,
}

/// Errors produced by semantic extraction.
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    #[error("semantic analysis failed for {path}: {reason}")]
    Analysis { path: PathBuf, reason: String },

    #[error("semantic runtime unavailable: {reason}")]
    RuntimeUnavailable { reason: String },

    #[error("unsupported language: {language}")]
    Unsupported { language: String },
}

// ---------------------------------------------------------------------------
// SemanticBackend trait
// ---------------------------------------------------------------------------

/// Contract for a semantic enrichment backend.
///
/// Unlike syntax backends, semantic backends may require external runtimes
/// (e.g. a TypeScript language server process). Lifecycle management for
/// these runtimes is the backend's responsibility.
///
/// Semantic backends receive the merged syntax baseline (not individual
/// per-backend extractions) so they can enrich rather than duplicate
/// syntax extraction work.
pub trait SemanticBackend: Send + Sync {
    /// The language this backend handles.
    fn language(&self) -> &str;

    /// Describes what this backend can produce.
    fn capability(&self) -> &SemanticCapability;

    /// Enrich or produce symbols for a file.
    ///
    /// `syntax_baseline` is the merged result of all syntax backends for
    /// this file. It is `None` when no syntax backend was available or all
    /// syntax backends failed (transitional semantic-only path).
    fn enrich_symbols(
        &self,
        file: &PreparedFile,
        syntax_baseline: Option<&SyntaxMergeBaseline>,
    ) -> Result<SemanticExtraction, SemanticError>;
}

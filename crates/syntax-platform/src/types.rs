use std::path::PathBuf;

use core_model::{BackendId, SourceSpan, SymbolKind};

/// Describes the symbol kinds and features a syntax backend supports.
#[derive(Debug, Clone)]
pub struct SyntaxCapability {
    /// Symbol kinds this backend can extract.
    pub supported_kinds: Vec<SymbolKind>,
    /// Whether this backend extracts parent/container relationships.
    pub supports_containers: bool,
    /// Whether this backend extracts doc comments.
    pub supports_docs: bool,
}

/// A symbol extracted by a syntax backend.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
}

/// Result of syntax extraction for a single file.
#[derive(Debug, Clone)]
pub struct SyntaxExtraction {
    pub language: String,
    pub symbols: Vec<SyntaxSymbol>,
    pub backend_id: BackendId,
}

/// Errors produced by syntax extraction.
#[derive(Debug, thiserror::Error)]
pub enum SyntaxError {
    #[error("parse failed for {path}: {reason}")]
    Parse { path: PathBuf, reason: String },

    #[error("unsupported language: {language}")]
    Unsupported { language: String },
}

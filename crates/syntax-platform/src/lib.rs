//! Multi-language syntax indexing platform.
//!
//! Provides tree-sitter-backed syntax extraction for CodeAtlas. Each
//! language is defined by a [`LanguageProfile`](languages::LanguageProfile)
//! and produces [`SyntaxExtraction`] output via
//! the [`SyntaxBackend`] trait.

pub mod extraction;
pub mod languages;
pub mod types;

use std::path::PathBuf;

use core_model::BackendId;
pub use types::{SyntaxCapability, SyntaxError, SyntaxExtraction, SyntaxSymbol};

// ---------------------------------------------------------------------------
// PreparedFile
// ---------------------------------------------------------------------------

/// A file ready for backend processing. Produced by the discovery stage.
#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub language: String,
    pub content: Vec<u8>,
}

// ---------------------------------------------------------------------------
// SyntaxMergeBaseline
// ---------------------------------------------------------------------------

/// Merged result of all syntax backends for a single file.
///
/// This is the canonical syntax-derived symbol set that semantic backends
/// receive as input. It is produced by the merge engine's syntax-merge
/// phase and represents one consistent view of the file's symbols, even
/// when multiple syntax backends contributed.
#[derive(Debug, Clone)]
pub struct SyntaxMergeBaseline {
    pub language: String,
    pub symbols: Vec<SyntaxSymbol>,
    /// Backend IDs that contributed to this baseline.
    pub contributing_backends: Vec<BackendId>,
}

// ---------------------------------------------------------------------------
// SyntaxBackend trait
// ---------------------------------------------------------------------------

/// Contract for a syntax extraction backend.
///
/// Implementations are deterministic: the same file content always produces
/// the same extraction result. No external process or runtime is required.
pub trait SyntaxBackend: Send + Sync {
    /// The language this backend handles.
    fn language(&self) -> &str;

    /// Describes what this backend can extract.
    fn capability(&self) -> &SyntaxCapability;

    /// Extract symbols from a prepared file.
    ///
    /// The file's language must match `self.language()`. Returns
    /// `SyntaxError::Unsupported` if it does not.
    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError>;
}

// ---------------------------------------------------------------------------
// RustSyntaxBackend
// ---------------------------------------------------------------------------

use core_model::SymbolKind;
use tree_sitter::Parser;

/// Tree-sitter-based syntax backend for Rust.
pub struct RustSyntaxBackend {
    capability: SyntaxCapability,
}

impl RustSyntaxBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            capability: SyntaxCapability {
                supported_kinds: vec![
                    SymbolKind::Function,
                    SymbolKind::Method,
                    SymbolKind::Class,
                    SymbolKind::Type,
                    SymbolKind::Constant,
                ],
                supports_containers: true,
                supports_docs: true,
            },
        }
    }

    /// Returns the [`BackendId`] for this backend.
    #[must_use]
    pub fn backend_id() -> BackendId {
        BackendId("syntax-rust".to_string())
    }
}

impl Default for RustSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for RustSyntaxBackend {
    fn language(&self) -> &str {
        "rust"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "rust" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::rust::RUST_PROFILE;

        let mut parser = Parser::new();
        parser
            .set_language(&(profile.ts_language)())
            .map_err(|err| SyntaxError::Parse {
                path: file.relative_path.clone(),
                reason: format!("failed to set language: {err}"),
            })?;

        let tree = parser
            .parse(&file.content, None)
            .ok_or_else(|| SyntaxError::Parse {
                path: file.relative_path.clone(),
                reason: "tree-sitter parse returned no tree".to_string(),
            })?;

        let symbols = extraction::extract_symbols(tree.root_node(), &file.content, profile);

        Ok(SyntaxExtraction {
            language: "rust".to_string(),
            symbols,
            backend_id: Self::backend_id(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::SymbolKind;

    fn extract_rust(source: &str) -> SyntaxExtraction {
        let backend = RustSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("src/lib.rs"),
            absolute_path: PathBuf::from("/tmp/test/src/lib.rs"),
            language: "rust".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    fn find_symbol<'a>(symbols: &'a [SyntaxSymbol], name: &str) -> &'a SyntaxSymbol {
        symbols.iter().find(|s| s.name == name).unwrap_or_else(|| {
            let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
            panic!("symbol '{name}' not found in: {names:?}")
        })
    }

    // -- Backend identity --

    #[test]
    fn backend_id_follows_naming_convention() {
        let backend = RustSyntaxBackend::new();
        assert_eq!(backend.language(), "rust");
        assert_eq!(RustSyntaxBackend::backend_id().0, "syntax-rust");
    }

    #[test]
    fn capability_reports_expected_kinds() {
        let backend = RustSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Function));
        assert!(cap.supported_kinds.contains(&SymbolKind::Class));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn unsupported_language_returns_error() {
        let backend = RustSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.py"),
            absolute_path: PathBuf::from("/tmp/main.py"),
            language: "python".to_string(),
            content: b"print('hello')".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Extraction output --

    #[test]
    fn extraction_carries_backend_id() {
        let result = extract_rust("fn hello() {}\n");
        assert_eq!(result.backend_id.0, "syntax-rust");
        assert_eq!(result.language, "rust");
    }

    // -- Function extraction --

    #[test]
    fn extracts_free_function() {
        let result = extract_rust("fn hello() {}\n");
        assert_eq!(result.symbols.len(), 1);
        let sym = &result.symbols[0];
        assert_eq!(sym.name, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert_eq!(sym.span.start_line, 1);
        assert!(sym.span.byte_length > 0);
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn extracts_function_signature() {
        let result = extract_rust("pub fn process(input: &str) -> bool {\n    true\n}\n");
        let sym = find_symbol(&result.symbols, "process");
        assert_eq!(sym.signature, "pub fn process(input: &str) -> bool");
    }

    // -- Struct extraction --

    #[test]
    fn extracts_struct_as_class() {
        let result = extract_rust("struct Point {\n    x: f64,\n    y: f64,\n}\n");
        let sym = find_symbol(&result.symbols, "Point");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "Point");
    }

    // -- Impl methods --

    #[test]
    fn extracts_impl_methods_with_qualified_names() {
        let source = "struct Foo;\nimpl Foo {\n    fn bar() {}\n    fn baz() {}\n}\n";
        let result = extract_rust(source);
        let bar = find_symbol(&result.symbols, "bar");
        assert_eq!(bar.kind, SymbolKind::Method);
        assert_eq!(bar.qualified_name, "Foo::bar");
        assert_eq!(bar.parent_qualified_name.as_deref(), Some("Foo"));
    }

    #[test]
    fn extracts_impl_methods_for_generic_type() {
        let source = "struct Wrapper<T>(T);\nimpl<T> Wrapper<T> {\n    fn inner(&self) -> &T { &self.0 }\n}\n";
        let result = extract_rust(source);
        let sym = find_symbol(&result.symbols, "inner");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "Wrapper::inner");
    }

    // -- Enum, trait, const, type alias --

    #[test]
    fn extracts_enum_as_type() {
        let result = extract_rust("enum Color {\n    Red,\n    Green,\n}\n");
        let sym = find_symbol(&result.symbols, "Color");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn extracts_trait_as_type() {
        let result = extract_rust("trait Drawable {\n    fn draw(&self);\n}\n");
        let sym = find_symbol(&result.symbols, "Drawable");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn extracts_trait_method_declarations() {
        let source = "trait Drawable {\n    fn draw(&self);\n}\n";
        let result = extract_rust(source);
        let draw = find_symbol(&result.symbols, "draw");
        assert_eq!(draw.kind, SymbolKind::Method);
        assert_eq!(draw.qualified_name, "Drawable::draw");
    }

    #[test]
    fn extracts_const() {
        let result = extract_rust("const MAX_SIZE: usize = 100;\n");
        let sym = find_symbol(&result.symbols, "MAX_SIZE");
        assert_eq!(sym.kind, SymbolKind::Constant);
    }

    #[test]
    fn extracts_static() {
        let result = extract_rust("static INSTANCE: u32 = 0;\n");
        let sym = find_symbol(&result.symbols, "INSTANCE");
        assert_eq!(sym.kind, SymbolKind::Constant);
    }

    #[test]
    fn extracts_type_alias() {
        let result = extract_rust("type Result<T> = std::result::Result<T, Error>;\n");
        let sym = find_symbol(&result.symbols, "Result");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    // -- Docstrings --

    #[test]
    fn extracts_doc_comments() {
        let source = "/// Does something useful.\n/// With multiple lines.\nfn documented() {}\n";
        let result = extract_rust(source);
        let sym = find_symbol(&result.symbols, "documented");
        assert_eq!(
            sym.docstring.as_deref(),
            Some("Does something useful.\nWith multiple lines.")
        );
    }

    #[test]
    fn no_docstring_when_absent() {
        let result = extract_rust("fn bare() {}\n");
        let sym = find_symbol(&result.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    // -- Edge cases --

    #[test]
    fn empty_file_produces_no_symbols() {
        let result = extract_rust("");
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn whitespace_only_file_produces_no_symbols() {
        let result = extract_rust("   \n\n  \n");
        assert!(result.symbols.is_empty());
    }

    // -- Determinism --

    #[test]
    fn comprehensive_fixture_extraction_is_deterministic() {
        let source = r#"
/// Module-level constant.
const VERSION: &str = "1.0";

/// A point in 2D space.
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    /// Creates a new point.
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    fn origin() -> Self {
        Point::new(0.0, 0.0)
    }
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}

trait Area {
    fn area(&self) -> f64;
}

type Coord = f64;
"#;

        let output1 = extract_rust(source);
        let output2 = extract_rust(source);

        // Deterministic: same output on repeated runs.
        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        // Verify expected symbols.
        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("VERSION", SymbolKind::Constant)));
        assert!(names.contains(&("Point", SymbolKind::Class)));
        assert!(names.contains(&("new", SymbolKind::Method)));
        assert!(names.contains(&("origin", SymbolKind::Method)));
        assert!(names.contains(&("Shape", SymbolKind::Type)));
        assert!(names.contains(&("Area", SymbolKind::Type)));
        assert!(names.contains(&("area", SymbolKind::Method)));
        assert!(names.contains(&("Coord", SymbolKind::Type)));

        // Verify docstrings on documented items.
        let version = find_symbol(&output1.symbols, "VERSION");
        assert_eq!(version.docstring.as_deref(), Some("Module-level constant."));
        let new = find_symbol(&output1.symbols, "new");
        assert_eq!(new.docstring.as_deref(), Some("Creates a new point."));
        assert_eq!(new.parent_qualified_name.as_deref(), Some("Point"));
    }
}

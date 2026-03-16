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
// GoSyntaxBackend
// ---------------------------------------------------------------------------

/// Tree-sitter-based syntax backend for Go.
pub struct GoSyntaxBackend {
    capability: SyntaxCapability,
}

impl GoSyntaxBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            capability: SyntaxCapability {
                supported_kinds: vec![
                    SymbolKind::Function,
                    SymbolKind::Method,
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
        BackendId("syntax-go".to_string())
    }
}

impl Default for GoSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for GoSyntaxBackend {
    fn language(&self) -> &str {
        "go"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "go" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::go::GO_PROFILE;

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
            language: "go".to_string(),
            symbols,
            backend_id: Self::backend_id(),
        })
    }
}

// ---------------------------------------------------------------------------
// JavaSyntaxBackend
// ---------------------------------------------------------------------------

/// Tree-sitter-based syntax backend for Java.
pub struct JavaSyntaxBackend {
    capability: SyntaxCapability,
}

impl JavaSyntaxBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            capability: SyntaxCapability {
                supported_kinds: vec![
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
        BackendId("syntax-java".to_string())
    }
}

impl Default for JavaSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for JavaSyntaxBackend {
    fn language(&self) -> &str {
        "java"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "java" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::java::JAVA_PROFILE;

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
            language: "java".to_string(),
            symbols,
            backend_id: Self::backend_id(),
        })
    }
}

// ---------------------------------------------------------------------------
// JavaScriptSyntaxBackend
// ---------------------------------------------------------------------------

/// Tree-sitter-based syntax backend for JavaScript.
pub struct JavaScriptSyntaxBackend {
    capability: SyntaxCapability,
}

impl JavaScriptSyntaxBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            capability: SyntaxCapability {
                supported_kinds: vec![SymbolKind::Function, SymbolKind::Method, SymbolKind::Class],
                supports_containers: true,
                supports_docs: true,
            },
        }
    }

    /// Returns the [`BackendId`] for this backend.
    #[must_use]
    pub fn backend_id() -> BackendId {
        BackendId("syntax-javascript".to_string())
    }
}

impl Default for JavaScriptSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for JavaScriptSyntaxBackend {
    fn language(&self) -> &str {
        "javascript"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "javascript" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::javascript::JAVASCRIPT_PROFILE;

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
            language: "javascript".to_string(),
            symbols,
            backend_id: Self::backend_id(),
        })
    }
}

// ---------------------------------------------------------------------------
// PhpSyntaxBackend
// ---------------------------------------------------------------------------

/// Tree-sitter-based syntax backend for PHP.
pub struct PhpSyntaxBackend {
    capability: SyntaxCapability,
}

impl PhpSyntaxBackend {
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
        BackendId("syntax-php".to_string())
    }
}

impl Default for PhpSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for PhpSyntaxBackend {
    fn language(&self) -> &str {
        "php"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "php" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::php::PHP_PROFILE;

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
            language: "php".to_string(),
            symbols,
            backend_id: Self::backend_id(),
        })
    }
}

// ---------------------------------------------------------------------------
// PythonSyntaxBackend
// ---------------------------------------------------------------------------

/// Tree-sitter-based syntax backend for Python.
pub struct PythonSyntaxBackend {
    capability: SyntaxCapability,
}

impl PythonSyntaxBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            capability: SyntaxCapability {
                supported_kinds: vec![SymbolKind::Function, SymbolKind::Method, SymbolKind::Class],
                supports_containers: true,
                supports_docs: true,
            },
        }
    }

    /// Returns the [`BackendId`] for this backend.
    #[must_use]
    pub fn backend_id() -> BackendId {
        BackendId("syntax-python".to_string())
    }
}

impl Default for PythonSyntaxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxBackend for PythonSyntaxBackend {
    fn language(&self) -> &str {
        "python"
    }

    fn capability(&self) -> &SyntaxCapability {
        &self.capability
    }

    fn extract_symbols(&self, file: &PreparedFile) -> Result<SyntaxExtraction, SyntaxError> {
        if file.language != "python" {
            return Err(SyntaxError::Unsupported {
                language: file.language.clone(),
            });
        }

        let profile = &languages::python::PYTHON_PROFILE;

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
            language: "python".to_string(),
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

    // ===================================================================
    // PHP tests
    // ===================================================================

    fn extract_php(source: &str) -> SyntaxExtraction {
        let backend = PhpSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("app/Models/User.php"),
            absolute_path: PathBuf::from("/tmp/test/app/Models/User.php"),
            language: "php".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    // -- PHP backend identity --

    #[test]
    fn php_backend_id_follows_naming_convention() {
        let backend = PhpSyntaxBackend::new();
        assert_eq!(backend.language(), "php");
        assert_eq!(PhpSyntaxBackend::backend_id().0, "syntax-php");
    }

    #[test]
    fn php_capability_reports_expected_kinds() {
        let backend = PhpSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Function));
        assert!(cap.supported_kinds.contains(&SymbolKind::Method));
        assert!(cap.supported_kinds.contains(&SymbolKind::Class));
        assert!(cap.supported_kinds.contains(&SymbolKind::Type));
        assert!(cap.supported_kinds.contains(&SymbolKind::Constant));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn php_unsupported_language_returns_error() {
        let backend = PhpSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.rs"),
            absolute_path: PathBuf::from("/tmp/main.rs"),
            language: "rust".to_string(),
            content: b"fn main() {}".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- PHP extraction output --

    #[test]
    fn php_extraction_carries_backend_id() {
        let result = extract_php("<?php\nfunction hello() {}\n");
        assert_eq!(result.backend_id.0, "syntax-php");
        assert_eq!(result.language, "php");
    }

    // -- PHP function extraction --

    #[test]
    fn php_extracts_free_function() {
        let result = extract_php("<?php\nfunction hello(): void {}\n");
        let sym = find_symbol(&result.symbols, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn php_extracts_function_signature() {
        let result =
            extract_php("<?php\nfunction process(string $input): bool\n{\n    return true;\n}\n");
        let sym = find_symbol(&result.symbols, "process");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("function process"));
        assert!(sym.signature.contains("string $input"));
    }

    // -- PHP class extraction --

    #[test]
    fn php_extracts_class() {
        let result = extract_php("<?php\nclass User\n{\n}\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "User");
    }

    #[test]
    fn php_extracts_class_with_extends() {
        let result = extract_php("<?php\nclass User extends Model\n{\n}\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert!(sym.signature.contains("class User extends Model"));
    }

    // -- PHP method extraction --

    #[test]
    fn php_extracts_methods_with_qualified_names() {
        let source = "<?php\nclass User\n{\n    public function getName(): string\n    {\n        return $this->name;\n    }\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::getName");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("User"));
    }

    #[test]
    fn php_extracts_static_method() {
        let source = "<?php\nclass User\n{\n    public static function find(int $id): ?self\n    {\n        return null;\n    }\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "find");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::find");
    }

    // -- PHP interface extraction --

    #[test]
    fn php_extracts_interface() {
        let source = "<?php\ninterface Authenticatable\n{\n    public function getAuthIdentifier(): string;\n}\n";
        let result = extract_php(source);
        let iface = find_symbol(&result.symbols, "Authenticatable");
        assert_eq!(iface.kind, SymbolKind::Type);

        let method = find_symbol(&result.symbols, "getAuthIdentifier");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Authenticatable::getAuthIdentifier");
    }

    // -- PHP trait extraction --

    #[test]
    fn php_extracts_trait() {
        let source =
            "<?php\ntrait HasTimestamps\n{\n    public function touch(): void\n    {\n    }\n}\n";
        let result = extract_php(source);
        let t = find_symbol(&result.symbols, "HasTimestamps");
        assert_eq!(t.kind, SymbolKind::Type);

        let method = find_symbol(&result.symbols, "touch");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "HasTimestamps::touch");
    }

    // -- PHP enum extraction --

    #[test]
    fn php_extracts_enum() {
        let source = "<?php\nenum Status: string\n{\n    case Active = 'active';\n    case Inactive = 'inactive';\n\n    public function label(): string\n    {\n        return $this->value;\n    }\n}\n";
        let result = extract_php(source);
        let e = find_symbol(&result.symbols, "Status");
        assert_eq!(e.kind, SymbolKind::Type);

        let method = find_symbol(&result.symbols, "label");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Status::label");
    }

    // -- PHP constant extraction --

    #[test]
    fn php_extracts_top_level_constant() {
        let result = extract_php("<?php\nconst APP_VERSION = '1.0';\n");
        let sym = find_symbol(&result.symbols, "APP_VERSION");
        assert_eq!(sym.kind, SymbolKind::Constant);
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn php_extracts_class_constant() {
        let source = "<?php\nclass User\n{\n    const TABLE = 'users';\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "TABLE");
        assert_eq!(sym.kind, SymbolKind::Constant);
        assert_eq!(sym.qualified_name, "User::TABLE");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("User"));
    }

    // -- PHP docstrings --

    #[test]
    fn php_extracts_phpdoc_comment() {
        let source = "<?php\n/**\n * Represents a user in the system.\n */\nclass User\n{\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "User");
        assert!(sym.docstring.is_some(), "expected PHPDoc on User");
        let doc = sym.docstring.as_deref().unwrap();
        assert!(doc.contains("Represents a user"), "unexpected doc: {doc:?}");
    }

    #[test]
    fn php_extracts_method_phpdoc() {
        let source = "<?php\nclass User\n{\n    /**\n     * Get the user name.\n     */\n    public function getName(): string\n    {\n        return $this->name;\n    }\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert!(sym.docstring.is_some(), "expected PHPDoc on getName");
        let doc = sym.docstring.as_deref().unwrap();
        assert!(doc.contains("Get the user name"), "unexpected doc: {doc:?}");
    }

    #[test]
    fn php_no_docstring_when_absent() {
        let result = extract_php("<?php\nfunction bare() {}\n");
        let sym = find_symbol(&result.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    // -- PHP edge cases --

    #[test]
    fn php_empty_file_produces_no_symbols() {
        let result = extract_php("<?php\n");
        assert!(result.symbols.is_empty());
    }

    // -- PHP namespace behavior --

    #[test]
    fn php_statement_namespace_qualifies_class() {
        let source = "<?php\nnamespace App\\Models;\nclass User\n{\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "App::Models::User");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("App::Models"));
    }

    #[test]
    fn php_statement_namespace_qualifies_methods() {
        let source = "<?php\nnamespace App\\Models;\nclass User\n{\n    public function getName(): string\n    {\n        return $this->name;\n    }\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert_eq!(sym.qualified_name, "App::Models::User::getName");
        assert_eq!(
            sym.parent_qualified_name.as_deref(),
            Some("App::Models::User")
        );
    }

    #[test]
    fn php_statement_namespace_qualifies_constants() {
        let source = "<?php\nnamespace App\\Config;\nconst VERSION = '1.0';\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "VERSION");
        assert_eq!(sym.qualified_name, "App::Config::VERSION");
    }

    #[test]
    fn php_statement_namespace_qualifies_functions() {
        let source =
            "<?php\nnamespace App\\Helpers;\nfunction config(): mixed\n{\n    return null;\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "config");
        assert_eq!(sym.qualified_name, "App::Helpers::config");
    }

    #[test]
    fn php_no_namespace_produces_unqualified_names() {
        let source =
            "<?php\nclass User\n{\n    public function getName(): string { return ''; }\n}\n";
        let result = extract_php(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert_eq!(sym.qualified_name, "User::getName");
    }

    // -- PHP Laravel-oriented comprehensive fixture --

    #[test]
    fn php_laravel_controller_fixture() {
        let source = r#"<?php

namespace App\Http\Controllers;

use App\Models\User;
use Illuminate\Http\Request;

/**
 * Handles user-related HTTP requests.
 */
class UserController extends Controller
{
    /**
     * Display a listing of users.
     */
    public function index(): JsonResponse
    {
        return response()->json(User::all());
    }

    public function show(int $id): JsonResponse
    {
        return response()->json(User::findOrFail($id));
    }

    public function store(Request $request): JsonResponse
    {
        $user = User::create($request->validated());
        return response()->json($user, 201);
    }
}
"#;
        let result = extract_php(source);

        let controller = find_symbol(&result.symbols, "UserController");
        assert_eq!(controller.kind, SymbolKind::Class);
        assert_eq!(
            controller.qualified_name,
            "App::Http::Controllers::UserController"
        );
        assert!(controller.docstring.is_some());

        let index = find_symbol(&result.symbols, "index");
        assert_eq!(index.kind, SymbolKind::Method);
        assert_eq!(
            index.qualified_name,
            "App::Http::Controllers::UserController::index"
        );
        assert!(index.docstring.is_some());

        let show = find_symbol(&result.symbols, "show");
        assert_eq!(show.kind, SymbolKind::Method);
        assert_eq!(
            show.qualified_name,
            "App::Http::Controllers::UserController::show"
        );

        let store = find_symbol(&result.symbols, "store");
        assert_eq!(store.kind, SymbolKind::Method);
        assert_eq!(
            store.qualified_name,
            "App::Http::Controllers::UserController::store"
        );
    }

    #[test]
    fn php_laravel_model_fixture() {
        let source = r#"<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

/**
 * Eloquent model for the posts table.
 */
class Post extends Model
{
    const STATUS_DRAFT = 'draft';
    const STATUS_PUBLISHED = 'published';

    public function comments(): HasMany
    {
        return $this->hasMany(Comment::class);
    }

    public function publish(): void
    {
        $this->status = self::STATUS_PUBLISHED;
        $this->save();
    }
}
"#;
        let result = extract_php(source);

        let post = find_symbol(&result.symbols, "Post");
        assert_eq!(post.kind, SymbolKind::Class);
        assert_eq!(post.qualified_name, "App::Models::Post");

        let draft = find_symbol(&result.symbols, "STATUS_DRAFT");
        assert_eq!(draft.kind, SymbolKind::Constant);
        assert_eq!(draft.qualified_name, "App::Models::Post::STATUS_DRAFT");

        let published = find_symbol(&result.symbols, "STATUS_PUBLISHED");
        assert_eq!(published.kind, SymbolKind::Constant);
        assert_eq!(
            published.qualified_name,
            "App::Models::Post::STATUS_PUBLISHED"
        );

        let comments = find_symbol(&result.symbols, "comments");
        assert_eq!(comments.kind, SymbolKind::Method);
        assert_eq!(comments.qualified_name, "App::Models::Post::comments");

        let publish = find_symbol(&result.symbols, "publish");
        assert_eq!(publish.kind, SymbolKind::Method);
        assert_eq!(publish.qualified_name, "App::Models::Post::publish");
    }

    #[test]
    fn php_laravel_artisan_command_fixture() {
        let source = r#"<?php

namespace App\Console\Commands;

use Illuminate\Console\Command;

class SyncUsers extends Command
{
    public function handle(): int
    {
        $this->info('Syncing users...');
        return self::SUCCESS;
    }
}
"#;
        let result = extract_php(source);

        let cmd = find_symbol(&result.symbols, "SyncUsers");
        assert_eq!(cmd.kind, SymbolKind::Class);
        assert_eq!(cmd.qualified_name, "App::Console::Commands::SyncUsers");

        let handle = find_symbol(&result.symbols, "handle");
        assert_eq!(handle.kind, SymbolKind::Method);
        assert_eq!(
            handle.qualified_name,
            "App::Console::Commands::SyncUsers::handle"
        );
    }

    // -- PHP determinism --

    #[test]
    fn php_comprehensive_extraction_is_deterministic() {
        let source = r#"<?php

namespace App\Foundation;

const APP_VERSION = '2.0';

/**
 * Application service container.
 */
class Container
{
    const SINGLETON = 'singleton';

    /**
     * Resolve a binding.
     */
    public function make(string $abstract): mixed
    {
        return null;
    }

    public static function getInstance(): static
    {
        return new static();
    }
}

interface ServiceProvider
{
    public function register(): void;
    public function boot(): void;
}

trait Macroable
{
    public static function macro(string $name, callable $fn): void
    {
    }
}

enum AppEnv: string
{
    case Production = 'production';
    case Staging = 'staging';
    case Local = 'local';
}

function config(string $key): mixed
{
    return null;
}
"#;

        let output1 = extract_php(source);
        let output2 = extract_php(source);

        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("APP_VERSION", SymbolKind::Constant)));
        assert!(names.contains(&("Container", SymbolKind::Class)));
        assert!(names.contains(&("SINGLETON", SymbolKind::Constant)));
        assert!(names.contains(&("make", SymbolKind::Method)));
        assert!(names.contains(&("getInstance", SymbolKind::Method)));
        assert!(names.contains(&("ServiceProvider", SymbolKind::Type)));
        assert!(names.contains(&("register", SymbolKind::Method)));
        assert!(names.contains(&("boot", SymbolKind::Method)));
        assert!(names.contains(&("Macroable", SymbolKind::Type)));
        assert!(names.contains(&("macro", SymbolKind::Method)));
        assert!(names.contains(&("AppEnv", SymbolKind::Type)));
        assert!(names.contains(&("config", SymbolKind::Function)));

        // Verify namespaced qualified names.
        let make = find_symbol(&output1.symbols, "make");
        assert_eq!(make.qualified_name, "App::Foundation::Container::make");
        assert_eq!(
            make.parent_qualified_name.as_deref(),
            Some("App::Foundation::Container")
        );

        // Verify docstrings.
        let container = find_symbol(&output1.symbols, "Container");
        assert!(container.docstring.is_some());

        let make_doc = find_symbol(&output1.symbols, "make");
        assert!(make_doc.docstring.is_some());
    }

    // ===================================================================
    // Python tests
    // ===================================================================

    fn extract_python(source: &str) -> SyntaxExtraction {
        let backend = PythonSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("app/models.py"),
            absolute_path: PathBuf::from("/tmp/test/app/models.py"),
            language: "python".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    // -- Python backend identity --

    #[test]
    fn python_backend_id_follows_naming_convention() {
        let backend = PythonSyntaxBackend::new();
        assert_eq!(backend.language(), "python");
        assert_eq!(PythonSyntaxBackend::backend_id().0, "syntax-python");
    }

    #[test]
    fn python_capability_reports_expected_kinds() {
        let backend = PythonSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Function));
        assert!(cap.supported_kinds.contains(&SymbolKind::Method));
        assert!(cap.supported_kinds.contains(&SymbolKind::Class));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn python_unsupported_language_returns_error() {
        let backend = PythonSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.rs"),
            absolute_path: PathBuf::from("/tmp/main.rs"),
            language: "rust".to_string(),
            content: b"fn main() {}".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Python extraction output --

    #[test]
    fn python_extraction_carries_backend_id() {
        let result = extract_python("def hello():\n    pass\n");
        assert_eq!(result.backend_id.0, "syntax-python");
        assert_eq!(result.language, "python");
    }

    // -- Python function extraction --

    #[test]
    fn python_extracts_free_function() {
        let result = extract_python("def hello():\n    pass\n");
        let sym = find_symbol(&result.symbols, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn python_extracts_function_signature() {
        let result = extract_python("def process(x: int, y: str) -> bool:\n    return True\n");
        let sym = find_symbol(&result.symbols, "process");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("def process"));
        assert!(sym.signature.contains("x: int"));
    }

    #[test]
    fn python_extracts_async_function() {
        let result = extract_python("async def fetch(url: str) -> dict:\n    return {}\n");
        let sym = find_symbol(&result.symbols, "fetch");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    // -- Python class extraction --

    #[test]
    fn python_extracts_class() {
        let result = extract_python("class User:\n    pass\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "User");
    }

    #[test]
    fn python_extracts_class_with_base() {
        let result = extract_python("class Admin(User):\n    pass\n");
        let sym = find_symbol(&result.symbols, "Admin");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert!(sym.signature.contains("class Admin"));
    }

    // -- Python method extraction --

    #[test]
    fn python_extracts_methods_with_qualified_names() {
        let source = "class User:\n    def get_name(self) -> str:\n        return self.name\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "get_name");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::get_name");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("User"));
    }

    #[test]
    fn python_extracts_init_as_method() {
        let source = "class User:\n    def __init__(self, name: str):\n        self.name = name\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "__init__");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::__init__");
    }

    #[test]
    fn python_extracts_decorated_method() {
        let source =
            "class User:\n    @staticmethod\n    def create(name: str) -> 'User':\n        return User(name)\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "create");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::create");
    }

    #[test]
    fn python_extracts_property_as_method() {
        let source =
            "class User:\n    @property\n    def display_name(self) -> str:\n        return self.name\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "display_name");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::display_name");
    }

    // -- Python docstrings --

    #[test]
    fn python_extracts_function_docstring() {
        let source =
            "def helper(x: int) -> int:\n    \"\"\"A helper function.\"\"\"\n    return x + 1\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "helper");
        assert!(sym.docstring.is_some(), "expected docstring");
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("A helper function"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    #[test]
    fn python_extracts_class_docstring() {
        let source = "class User:\n    \"\"\"Represents a user.\"\"\"\n    pass\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "User");
        assert!(sym.docstring.is_some(), "expected class docstring");
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("Represents a user"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    #[test]
    fn python_extracts_method_docstring() {
        let source = "class User:\n    def get_name(self) -> str:\n        \"\"\"Return the user's name.\"\"\"\n        return self.name\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "get_name");
        assert!(sym.docstring.is_some(), "expected method docstring");
    }

    #[test]
    fn python_no_docstring_when_absent() {
        let result = extract_python("def bare():\n    pass\n");
        let sym = find_symbol(&result.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    #[test]
    fn python_single_quote_docstring() {
        let source = "def helper():\n    '''Single quoted doc.'''\n    pass\n";
        let result = extract_python(source);
        let sym = find_symbol(&result.symbols, "helper");
        assert!(sym.docstring.is_some());
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("Single quoted doc"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    // -- Python edge cases --

    #[test]
    fn python_empty_file_produces_no_symbols() {
        let result = extract_python("");
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn python_nested_class() {
        let source =
            "class Outer:\n    class Inner:\n        def method(self):\n            pass\n";
        let result = extract_python(source);

        let outer = find_symbol(&result.symbols, "Outer");
        assert_eq!(outer.kind, SymbolKind::Class);

        let inner = find_symbol(&result.symbols, "Inner");
        assert_eq!(inner.kind, SymbolKind::Class);
        assert_eq!(inner.qualified_name, "Outer::Inner");

        let method = find_symbol(&result.symbols, "method");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Outer::Inner::method");
    }

    // -- Python representative fixture --

    #[test]
    fn python_django_style_model_fixture() {
        let source = r#"
class Article:
    """Represents an article in the system."""

    def __init__(self, title: str, body: str) -> None:
        """Initialize an article."""
        self.title = title
        self.body = body

    def publish(self) -> None:
        """Mark the article as published."""
        self.published = True

    @property
    def summary(self) -> str:
        return self.body[:100]

    @classmethod
    def from_dict(cls, data: dict) -> "Article":
        return cls(data["title"], data["body"])

class Comment:
    """A comment on an article."""

    def __init__(self, article: Article, text: str) -> None:
        self.article = article
        self.text = text

def create_article(title: str, body: str) -> Article:
    """Factory function for articles."""
    return Article(title, body)
"#;
        let result = extract_python(source);

        let article = find_symbol(&result.symbols, "Article");
        assert_eq!(article.kind, SymbolKind::Class);
        assert!(article.docstring.is_some());

        let init = find_symbol(&result.symbols, "__init__");
        assert_eq!(init.kind, SymbolKind::Method);
        assert_eq!(init.qualified_name, "Article::__init__");
        assert!(init.docstring.is_some());

        let publish = find_symbol(&result.symbols, "publish");
        assert_eq!(publish.kind, SymbolKind::Method);
        assert_eq!(publish.qualified_name, "Article::publish");

        let summary = find_symbol(&result.symbols, "summary");
        assert_eq!(summary.kind, SymbolKind::Method);
        assert_eq!(summary.qualified_name, "Article::summary");

        let from_dict = find_symbol(&result.symbols, "from_dict");
        assert_eq!(from_dict.kind, SymbolKind::Method);
        assert_eq!(from_dict.qualified_name, "Article::from_dict");

        let comment = find_symbol(&result.symbols, "Comment");
        assert_eq!(comment.kind, SymbolKind::Class);
        assert!(comment.docstring.is_some());

        let create = find_symbol(&result.symbols, "create_article");
        assert_eq!(create.kind, SymbolKind::Function);
        assert!(create.docstring.is_some());
    }

    #[test]
    fn python_comprehensive_extraction_is_deterministic() {
        let source = r#"
class Base:
    """Base class."""

    def method(self) -> None:
        """A method."""
        pass

class Child(Base):
    def override_method(self) -> None:
        pass

    @staticmethod
    def static_helper() -> int:
        return 0

def free_function(x: int) -> int:
    """A free function."""
    return x

async def async_function() -> None:
    pass
"#;

        let output1 = extract_python(source);
        let output2 = extract_python(source);

        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("Base", SymbolKind::Class)));
        assert!(names.contains(&("method", SymbolKind::Method)));
        assert!(names.contains(&("Child", SymbolKind::Class)));
        assert!(names.contains(&("override_method", SymbolKind::Method)));
        assert!(names.contains(&("static_helper", SymbolKind::Method)));
        assert!(names.contains(&("free_function", SymbolKind::Function)));
        assert!(names.contains(&("async_function", SymbolKind::Function)));

        let method = find_symbol(&output1.symbols, "method");
        assert_eq!(method.qualified_name, "Base::method");
        assert!(method.docstring.is_some());
    }

    // ===================================================================
    // Go tests
    // ===================================================================

    fn extract_go(source: &str) -> SyntaxExtraction {
        let backend = GoSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.go"),
            absolute_path: PathBuf::from("/tmp/test/main.go"),
            language: "go".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    // -- Go backend identity --

    #[test]
    fn go_backend_id_follows_naming_convention() {
        let backend = GoSyntaxBackend::new();
        assert_eq!(backend.language(), "go");
        assert_eq!(GoSyntaxBackend::backend_id().0, "syntax-go");
    }

    #[test]
    fn go_capability_reports_expected_kinds() {
        let backend = GoSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Function));
        assert!(cap.supported_kinds.contains(&SymbolKind::Method));
        assert!(cap.supported_kinds.contains(&SymbolKind::Type));
        assert!(cap.supported_kinds.contains(&SymbolKind::Constant));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn go_unsupported_language_returns_error() {
        let backend = GoSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.rs"),
            absolute_path: PathBuf::from("/tmp/main.rs"),
            language: "rust".to_string(),
            content: b"fn main() {}".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Go extraction output --

    #[test]
    fn go_extraction_carries_backend_id() {
        let result = extract_go("package main\nfunc hello() {}\n");
        assert_eq!(result.backend_id.0, "syntax-go");
        assert_eq!(result.language, "go");
    }

    // -- Go function extraction --

    #[test]
    fn go_extracts_free_function() {
        let result = extract_go("package main\nfunc hello() {}\n");
        let sym = find_symbol(&result.symbols, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn go_extracts_function_signature() {
        let result =
            extract_go("package main\nfunc process(x int, y string) bool {\n    return true\n}\n");
        let sym = find_symbol(&result.symbols, "process");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("func process"));
        assert!(sym.signature.contains("x int"));
    }

    // -- Go method extraction --

    #[test]
    fn go_extracts_pointer_receiver_method() {
        let source = "package main\ntype Config struct{}\nfunc (c *Config) GetName() string {\n    return c.Name\n}\n";
        let result = extract_go(source);
        let sym = find_symbol(&result.symbols, "GetName");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "Config::GetName");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("Config"));
    }

    #[test]
    fn go_extracts_value_receiver_method() {
        let source = "package main\ntype Config struct{}\nfunc (c Config) String() string {\n    return c.Name\n}\n";
        let result = extract_go(source);
        let sym = find_symbol(&result.symbols, "String");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "Config::String");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("Config"));
    }

    // -- Go type extraction --

    #[test]
    fn go_extracts_struct_type() {
        let result = extract_go("package main\ntype Config struct {\n    Name string\n}\n");
        let sym = find_symbol(&result.symbols, "Config");
        assert_eq!(sym.kind, SymbolKind::Type);
        assert_eq!(sym.qualified_name, "Config");
    }

    #[test]
    fn go_extracts_interface_type() {
        let result = extract_go(
            "package main\ntype Handler interface {\n    Handle(input string) error\n}\n",
        );
        let sym = find_symbol(&result.symbols, "Handler");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn go_extracts_type_alias() {
        let result = extract_go("package main\ntype Color int\n");
        let sym = find_symbol(&result.symbols, "Color");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    // -- Go constant extraction --

    #[test]
    fn go_extracts_single_constant() {
        let result = extract_go("package main\nconst MaxSize = 100\n");
        let sym = find_symbol(&result.symbols, "MaxSize");
        assert_eq!(sym.kind, SymbolKind::Constant);
    }

    #[test]
    fn go_extracts_grouped_constants() {
        let result = extract_go("package main\nconst (\n    Red = iota\n    Green\n    Blue\n)\n");
        let red = find_symbol(&result.symbols, "Red");
        assert_eq!(red.kind, SymbolKind::Constant);
        let green = find_symbol(&result.symbols, "Green");
        assert_eq!(green.kind, SymbolKind::Constant);
        let blue = find_symbol(&result.symbols, "Blue");
        assert_eq!(blue.kind, SymbolKind::Constant);
    }

    // -- Go docstrings --

    #[test]
    fn go_extracts_doc_comment() {
        let source =
            "package main\n// NewConfig creates a new Config.\nfunc NewConfig() *Config {\n    return nil\n}\n";
        let result = extract_go(source);
        let sym = find_symbol(&result.symbols, "NewConfig");
        assert!(sym.docstring.is_some(), "expected Go doc comment");
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("NewConfig creates"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    #[test]
    fn go_extracts_multiline_doc_comment() {
        let source = "package main\n// GetName returns the name.\n// It is safe to call concurrently.\nfunc GetName() string {\n    return \"\"\n}\n";
        let result = extract_go(source);
        let sym = find_symbol(&result.symbols, "GetName");
        assert!(sym.docstring.is_some());
        let doc = sym.docstring.as_deref().unwrap();
        assert!(doc.contains("GetName returns"));
        assert!(doc.contains("safe to call"));
    }

    #[test]
    fn go_no_docstring_when_absent() {
        let result = extract_go("package main\nfunc bare() {}\n");
        let sym = find_symbol(&result.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    // -- Go edge cases --

    #[test]
    fn go_empty_file_produces_no_symbols() {
        let result = extract_go("package main\n");
        assert!(result.symbols.is_empty());
    }

    // -- Go representative fixture --

    #[test]
    fn go_http_handler_fixture() {
        let source = r#"package handlers

import "net/http"

// Server holds HTTP server configuration.
type Server struct {
    Addr string
    Port int
}

// Handler defines the HTTP handler interface.
type Handler interface {
    ServeHTTP(w http.ResponseWriter, r *http.Request)
}

// NewServer creates a new server.
func NewServer(addr string, port int) *Server {
    return &Server{Addr: addr, Port: port}
}

// Start starts the server.
func (s *Server) Start() error {
    return nil
}

// Stop gracefully shuts down the server.
func (s *Server) Stop() error {
    return nil
}

const DefaultPort = 8080
"#;
        let result = extract_go(source);

        let server = find_symbol(&result.symbols, "Server");
        assert_eq!(server.kind, SymbolKind::Type);
        assert!(server.docstring.is_some());

        let handler = find_symbol(&result.symbols, "Handler");
        assert_eq!(handler.kind, SymbolKind::Type);
        assert!(handler.docstring.is_some());

        let new_server = find_symbol(&result.symbols, "NewServer");
        assert_eq!(new_server.kind, SymbolKind::Function);
        assert!(new_server.docstring.is_some());

        let start = find_symbol(&result.symbols, "Start");
        assert_eq!(start.kind, SymbolKind::Method);
        assert_eq!(start.qualified_name, "Server::Start");
        assert!(start.docstring.is_some());

        let stop = find_symbol(&result.symbols, "Stop");
        assert_eq!(stop.kind, SymbolKind::Method);
        assert_eq!(stop.qualified_name, "Server::Stop");

        let port = find_symbol(&result.symbols, "DefaultPort");
        assert_eq!(port.kind, SymbolKind::Constant);
    }

    #[test]
    fn go_comprehensive_extraction_is_deterministic() {
        let source = r#"package main

// Config holds configuration.
type Config struct {
    Name string
}

type Handler interface {
    Handle() error
}

type Status int

const MaxRetries = 3

func NewConfig(name string) *Config {
    return &Config{Name: name}
}

func (c *Config) GetName() string {
    return c.Name
}

func helper(x int) int {
    return x + 1
}
"#;

        let output1 = extract_go(source);
        let output2 = extract_go(source);

        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("Config", SymbolKind::Type)));
        assert!(names.contains(&("Handler", SymbolKind::Type)));
        assert!(names.contains(&("Status", SymbolKind::Type)));
        assert!(names.contains(&("MaxRetries", SymbolKind::Constant)));
        assert!(names.contains(&("NewConfig", SymbolKind::Function)));
        assert!(names.contains(&("GetName", SymbolKind::Method)));
        assert!(names.contains(&("helper", SymbolKind::Function)));

        let get_name = find_symbol(&output1.symbols, "GetName");
        assert_eq!(get_name.qualified_name, "Config::GetName");
        assert_eq!(get_name.parent_qualified_name.as_deref(), Some("Config"));

        let config = find_symbol(&output1.symbols, "Config");
        assert!(config.docstring.is_some());
    }

    // ===================================================================
    // Java tests
    // ===================================================================

    fn extract_java(source: &str) -> SyntaxExtraction {
        let backend = JavaSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("src/main/java/User.java"),
            absolute_path: PathBuf::from("/tmp/test/src/main/java/User.java"),
            language: "java".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    // -- Java backend identity --

    #[test]
    fn java_backend_id_follows_naming_convention() {
        let backend = JavaSyntaxBackend::new();
        assert_eq!(backend.language(), "java");
        assert_eq!(JavaSyntaxBackend::backend_id().0, "syntax-java");
    }

    #[test]
    fn java_capability_reports_expected_kinds() {
        let backend = JavaSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Method));
        assert!(cap.supported_kinds.contains(&SymbolKind::Class));
        assert!(cap.supported_kinds.contains(&SymbolKind::Type));
        assert!(cap.supported_kinds.contains(&SymbolKind::Constant));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn java_unsupported_language_returns_error() {
        let backend = JavaSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.py"),
            absolute_path: PathBuf::from("/tmp/main.py"),
            language: "python".to_string(),
            content: b"def foo(): pass".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- Java extraction output --

    #[test]
    fn java_extraction_carries_backend_id() {
        let result = extract_java("public class Foo {}\n");
        assert_eq!(result.backend_id.0, "syntax-java");
        assert_eq!(result.language, "java");
    }

    // -- Java class extraction --

    #[test]
    fn java_extracts_class() {
        let result = extract_java("public class User {}\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "User");
    }

    // -- Java package behavior --

    #[test]
    fn java_package_qualifies_class() {
        let source = "package com.example.app;\npublic class User {}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "com::example::app::User");
        assert_eq!(
            sym.parent_qualified_name.as_deref(),
            Some("com::example::app")
        );
    }

    #[test]
    fn java_package_qualifies_methods() {
        let source = "package com.example;\npublic class Svc {\n    public void run() {}\n}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "run");
        assert_eq!(sym.qualified_name, "com::example::Svc::run");
    }

    #[test]
    fn java_no_package_produces_unqualified_names() {
        let result = extract_java("public class User {}\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.qualified_name, "User");
    }

    // -- Java static final constant extraction --

    #[test]
    fn java_extracts_static_final_constant() {
        let source =
            "public class Config {\n    public static final String APP_NAME = \"myapp\";\n}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "APP_NAME");
        assert_eq!(sym.kind, SymbolKind::Constant);
        assert_eq!(sym.qualified_name, "Config::APP_NAME");
    }

    #[test]
    fn java_extracts_multiple_static_final_constants() {
        let source = "public class Limits {\n    public static final int MAX = 100;\n    public static final int MIN = 0;\n}\n";
        let result = extract_java(source);
        let max = find_symbol(&result.symbols, "MAX");
        assert_eq!(max.kind, SymbolKind::Constant);
        let min = find_symbol(&result.symbols, "MIN");
        assert_eq!(min.kind, SymbolKind::Constant);
    }

    #[test]
    fn java_skips_non_constant_fields() {
        let source = "public class User {\n    private String name;\n    protected int count;\n    public static final String TABLE = \"users\";\n}\n";
        let result = extract_java(source);
        // Only TABLE should be a Constant; name and count are regular fields.
        let constants: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(constants.len(), 1);
        assert_eq!(constants[0].name, "TABLE");
    }

    // -- Java method extraction --

    #[test]
    fn java_extracts_methods_with_qualified_names() {
        let source = "public class User {\n    public String getName() {\n        return this.name;\n    }\n}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::getName");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("User"));
    }

    #[test]
    fn java_extracts_constructor() {
        let source = "public class User {\n    public User(String name) {\n        this.name = name;\n    }\n}\n";
        let result = extract_java(source);
        // Both the class and constructor are named "User"
        let constructors: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.name == "User" && s.kind == SymbolKind::Method)
            .collect();
        assert_eq!(constructors.len(), 1);
        assert_eq!(constructors[0].qualified_name, "User::User");
    }

    #[test]
    fn java_extracts_static_method() {
        let source = "public class User {\n    public static User create(String name) {\n        return new User(name);\n    }\n}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "create");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::create");
    }

    // -- Java interface extraction --

    #[test]
    fn java_extracts_interface() {
        let source = "public interface Handler {\n    void handle(String input);\n}\n";
        let result = extract_java(source);
        let iface = find_symbol(&result.symbols, "Handler");
        assert_eq!(iface.kind, SymbolKind::Type);

        let method = find_symbol(&result.symbols, "handle");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Handler::handle");
    }

    // -- Java enum extraction --

    #[test]
    fn java_extracts_enum() {
        let source = "public enum Status {\n    ACTIVE,\n    INACTIVE;\n\n    public String label() {\n        return this.name();\n    }\n}\n";
        let result = extract_java(source);

        let e = find_symbol(&result.symbols, "Status");
        assert_eq!(e.kind, SymbolKind::Type);

        let active = find_symbol(&result.symbols, "ACTIVE");
        assert_eq!(active.kind, SymbolKind::Constant);
        assert_eq!(active.qualified_name, "Status::ACTIVE");

        let label = find_symbol(&result.symbols, "label");
        assert_eq!(label.kind, SymbolKind::Method);
        assert_eq!(label.qualified_name, "Status::label");
    }

    // -- Java record extraction --

    #[test]
    fn java_extracts_record() {
        let source = "public record Point(int x, int y) {\n    public double distance() {\n        return Math.sqrt(x * x + y * y);\n    }\n}\n";
        let result = extract_java(source);

        let point = find_symbol(&result.symbols, "Point");
        assert_eq!(point.kind, SymbolKind::Class);

        let distance = find_symbol(&result.symbols, "distance");
        assert_eq!(distance.kind, SymbolKind::Method);
        assert_eq!(distance.qualified_name, "Point::distance");
    }

    // -- Java docstrings --

    #[test]
    fn java_extracts_javadoc() {
        let source = "/**\n * Represents a user.\n */\npublic class User {}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "User");
        assert!(sym.docstring.is_some(), "expected Javadoc");
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("Represents a user"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    #[test]
    fn java_extracts_method_javadoc() {
        let source = "public class User {\n    /**\n     * Gets the name.\n     */\n    public String getName() {\n        return name;\n    }\n}\n";
        let result = extract_java(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert!(sym.docstring.is_some(), "expected method Javadoc");
    }

    #[test]
    fn java_no_docstring_when_absent() {
        let result = extract_java("public class Bare {}\n");
        let sym = find_symbol(&result.symbols, "Bare");
        assert!(sym.docstring.is_none());
    }

    // -- Java edge cases --

    #[test]
    fn java_empty_file_produces_no_symbols() {
        let result = extract_java("");
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn java_nested_class() {
        let source = "public class Outer {\n    public class Inner {\n        public void method() {}\n    }\n}\n";
        let result = extract_java(source);

        let outer = find_symbol(&result.symbols, "Outer");
        assert_eq!(outer.kind, SymbolKind::Class);

        let inner = find_symbol(&result.symbols, "Inner");
        assert_eq!(inner.kind, SymbolKind::Class);
        assert_eq!(inner.qualified_name, "Outer::Inner");

        let method = find_symbol(&result.symbols, "method");
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.qualified_name, "Outer::Inner::method");
    }

    // -- Java representative fixture --

    #[test]
    fn java_spring_style_controller_fixture() {
        let source = r#"
package com.example.controllers;

/**
 * Handles user-related HTTP requests.
 */
public class UserController {
    public static final String BASE_PATH = "/users";

    /**
     * List all users.
     */
    public List<User> index() {
        return userService.findAll();
    }

    public User show(Long id) {
        return userService.findById(id);
    }

    public User create(CreateUserRequest request) {
        return userService.create(request);
    }
}
"#;
        let result = extract_java(source);

        let controller = find_symbol(&result.symbols, "UserController");
        assert_eq!(controller.kind, SymbolKind::Class);
        assert_eq!(
            controller.qualified_name,
            "com::example::controllers::UserController"
        );
        assert!(controller.docstring.is_some());

        let index = find_symbol(&result.symbols, "index");
        assert_eq!(index.kind, SymbolKind::Method);
        assert_eq!(
            index.qualified_name,
            "com::example::controllers::UserController::index"
        );
        assert!(index.docstring.is_some());

        let base_path = find_symbol(&result.symbols, "BASE_PATH");
        assert_eq!(base_path.kind, SymbolKind::Constant);
        assert_eq!(
            base_path.qualified_name,
            "com::example::controllers::UserController::BASE_PATH"
        );

        let show = find_symbol(&result.symbols, "show");
        assert_eq!(show.kind, SymbolKind::Method);

        let create = find_symbol(&result.symbols, "create");
        assert_eq!(create.kind, SymbolKind::Method);
    }

    #[test]
    fn java_comprehensive_extraction_is_deterministic() {
        let source = r#"
package com.example;

/**
 * Application configuration.
 */
public class Config {
    public static final int VERSION = 1;

    /**
     * Creates a new config.
     */
    public Config(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }
}

public interface Service {
    void start();
    void stop();
}

public enum Priority {
    HIGH,
    MEDIUM,
    LOW;
}

public record Pair(Object first, Object second) {}
"#;

        let output1 = extract_java(source);
        let output2 = extract_java(source);

        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("Config", SymbolKind::Class)));
        assert!(names.contains(&("VERSION", SymbolKind::Constant)));
        assert!(names.contains(&("getName", SymbolKind::Method)));
        assert!(names.contains(&("Service", SymbolKind::Type)));
        assert!(names.contains(&("start", SymbolKind::Method)));
        assert!(names.contains(&("stop", SymbolKind::Method)));
        assert!(names.contains(&("Priority", SymbolKind::Type)));
        assert!(names.contains(&("HIGH", SymbolKind::Constant)));
        assert!(names.contains(&("Pair", SymbolKind::Class)));

        // Verify package-qualified names.
        let get_name = find_symbol(&output1.symbols, "getName");
        assert_eq!(get_name.qualified_name, "com::example::Config::getName");
        assert_eq!(
            get_name.parent_qualified_name.as_deref(),
            Some("com::example::Config")
        );

        let version = find_symbol(&output1.symbols, "VERSION");
        assert_eq!(version.qualified_name, "com::example::Config::VERSION");

        let config = find_symbol(&output1.symbols, "Config");
        assert!(config.docstring.is_some());
    }

    // ===================================================================
    // JavaScript tests
    // ===================================================================

    fn extract_js(source: &str) -> SyntaxExtraction {
        let backend = JavaScriptSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("src/index.js"),
            absolute_path: PathBuf::from("/tmp/test/src/index.js"),
            language: "javascript".to_string(),
            content: source.as_bytes().to_vec(),
        };
        backend.extract_symbols(&file).expect("extraction failed")
    }

    // -- JS backend identity --

    #[test]
    fn js_backend_id_follows_naming_convention() {
        let backend = JavaScriptSyntaxBackend::new();
        assert_eq!(backend.language(), "javascript");
        assert_eq!(JavaScriptSyntaxBackend::backend_id().0, "syntax-javascript");
    }

    #[test]
    fn js_capability_reports_expected_kinds() {
        let backend = JavaScriptSyntaxBackend::new();
        let cap = backend.capability();
        assert!(cap.supported_kinds.contains(&SymbolKind::Function));
        assert!(cap.supported_kinds.contains(&SymbolKind::Method));
        assert!(cap.supported_kinds.contains(&SymbolKind::Class));
        assert!(cap.supports_containers);
        assert!(cap.supports_docs);
    }

    #[test]
    fn js_unsupported_language_returns_error() {
        let backend = JavaScriptSyntaxBackend::new();
        let file = PreparedFile {
            relative_path: PathBuf::from("main.py"),
            absolute_path: PathBuf::from("/tmp/main.py"),
            language: "python".to_string(),
            content: b"def foo(): pass".to_vec(),
        };
        let err = backend.extract_symbols(&file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    // -- JS extraction output --

    #[test]
    fn js_extraction_carries_backend_id() {
        let result = extract_js("function hello() {}\n");
        assert_eq!(result.backend_id.0, "syntax-javascript");
        assert_eq!(result.language, "javascript");
    }

    // -- JS function extraction --

    #[test]
    fn js_extracts_free_function() {
        let result = extract_js("function hello() {}\n");
        let sym = find_symbol(&result.symbols, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn js_extracts_function_signature() {
        let result = extract_js("function process(x, y) {\n    return x + y;\n}\n");
        let sym = find_symbol(&result.symbols, "process");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("function process"));
    }

    #[test]
    fn js_extracts_async_function() {
        let result = extract_js("async function fetchData(url) {\n    return {};\n}\n");
        let sym = find_symbol(&result.symbols, "fetchData");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    // -- JS arrow function and function expression extraction --

    #[test]
    fn js_extracts_arrow_function() {
        let result = extract_js("const createApp = (config) => ({ config });\n");
        let sym = find_symbol(&result.symbols, "createApp");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "createApp");
    }

    #[test]
    fn js_extracts_async_arrow_function() {
        let result = extract_js("const fetchData = async (url) => { return {}; };\n");
        let sym = find_symbol(&result.symbols, "fetchData");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    #[test]
    fn js_extracts_function_expression() {
        let result = extract_js("const helper = function(x) { return x + 1; };\n");
        let sym = find_symbol(&result.symbols, "helper");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    #[test]
    fn js_skips_non_function_const() {
        let source = "const MAX = 3;\nconst config = { port: 8080 };\nconst items = [1, 2];\nconst createApp = () => ({});\n";
        let result = extract_js(source);
        // Only createApp should be extracted (arrow function).
        // MAX, config, items should NOT appear.
        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"createApp"),
            "missing createApp in {names:?}"
        );
        assert!(
            !names.contains(&"MAX"),
            "MAX should not be extracted: {names:?}"
        );
        assert!(
            !names.contains(&"config"),
            "config should not be extracted: {names:?}"
        );
        assert!(
            !names.contains(&"items"),
            "items should not be extracted: {names:?}"
        );
    }

    // -- JS class extraction --

    #[test]
    fn js_extracts_class() {
        let result = extract_js("class User {}\n");
        let sym = find_symbol(&result.symbols, "User");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "User");
    }

    #[test]
    fn js_extracts_class_with_extends() {
        let result = extract_js("class Admin extends User {}\n");
        let sym = find_symbol(&result.symbols, "Admin");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert!(sym.signature.contains("class Admin extends User"));
    }

    // -- JS method extraction --

    #[test]
    fn js_extracts_methods_with_qualified_names() {
        let source = "class User {\n    getName() {\n        return this.name;\n    }\n}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::getName");
        assert_eq!(sym.parent_qualified_name.as_deref(), Some("User"));
    }

    #[test]
    fn js_extracts_constructor() {
        let source = "class User {\n    constructor(name) {\n        this.name = name;\n    }\n}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "constructor");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::constructor");
    }

    #[test]
    fn js_extracts_static_method() {
        let source =
            "class User {\n    static create(name) {\n        return new User(name);\n    }\n}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "create");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::create");
    }

    #[test]
    fn js_extracts_getter() {
        let source = "class User {\n    get displayName() {\n        return this.name;\n    }\n}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "displayName");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "User::displayName");
    }

    // -- JS docstrings --

    #[test]
    fn js_extracts_jsdoc() {
        let source = "/**\n * Represents a user.\n */\nclass User {}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "User");
        assert!(sym.docstring.is_some(), "expected JSDoc");
        assert!(
            sym.docstring
                .as_deref()
                .unwrap()
                .contains("Represents a user"),
            "unexpected doc: {:?}",
            sym.docstring
        );
    }

    #[test]
    fn js_extracts_method_jsdoc() {
        let source = "class User {\n    /**\n     * Gets the name.\n     */\n    getName() {\n        return this.name;\n    }\n}\n";
        let result = extract_js(source);
        let sym = find_symbol(&result.symbols, "getName");
        assert!(sym.docstring.is_some(), "expected method JSDoc");
    }

    #[test]
    fn js_no_docstring_when_absent() {
        let result = extract_js("function bare() {}\n");
        let sym = find_symbol(&result.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    // -- JS edge cases --

    #[test]
    fn js_empty_file_produces_no_symbols() {
        let result = extract_js("");
        assert!(result.symbols.is_empty());
    }

    // -- JS representative fixture --

    #[test]
    fn js_express_style_fixture() {
        let source = r#"
/**
 * User management service.
 */
class UserService {
    constructor(db) {
        this.db = db;
    }

    /**
     * Find all users.
     */
    findAll() {
        return this.db.query('SELECT * FROM users');
    }

    findById(id) {
        return this.db.query('SELECT * FROM users WHERE id = ?', [id]);
    }

    static tableName() {
        return 'users';
    }
}

class AdminService extends UserService {
    promote(userId) {
        return this.db.query('UPDATE users SET role = ?', ['admin']);
    }
}

function createApp(config) {
    return { config };
}

const initDb = async (url) => {
    return {};
};

async function startServer(port) {
    return null;
}
"#;
        let result = extract_js(source);

        let svc = find_symbol(&result.symbols, "UserService");
        assert_eq!(svc.kind, SymbolKind::Class);
        assert!(svc.docstring.is_some());

        let constructor = find_symbol(&result.symbols, "constructor");
        assert_eq!(constructor.kind, SymbolKind::Method);
        assert_eq!(constructor.qualified_name, "UserService::constructor");

        let find_all = find_symbol(&result.symbols, "findAll");
        assert_eq!(find_all.kind, SymbolKind::Method);
        assert_eq!(find_all.qualified_name, "UserService::findAll");
        assert!(find_all.docstring.is_some());

        let admin = find_symbol(&result.symbols, "AdminService");
        assert_eq!(admin.kind, SymbolKind::Class);

        let promote = find_symbol(&result.symbols, "promote");
        assert_eq!(promote.kind, SymbolKind::Method);
        assert_eq!(promote.qualified_name, "AdminService::promote");

        let init_db = find_symbol(&result.symbols, "initDb");
        assert_eq!(init_db.kind, SymbolKind::Function);

        let create_app = find_symbol(&result.symbols, "createApp");
        assert_eq!(create_app.kind, SymbolKind::Function);

        let start_server = find_symbol(&result.symbols, "startServer");
        assert_eq!(start_server.kind, SymbolKind::Function);
    }

    #[test]
    fn js_comprehensive_extraction_is_deterministic() {
        let source = r#"
/**
 * Base class.
 */
class Base {
    constructor() {}
    method() {}
}

class Child extends Base {
    override() {}
    static helper() {}
}

function free(x) {
    return x;
}

async function asyncFn() {}
"#;

        let output1 = extract_js(source);
        let output2 = extract_js(source);

        assert_eq!(output1.symbols.len(), output2.symbols.len());
        for (a, b) in output1.symbols.iter().zip(output2.symbols.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
        }

        let names: Vec<(&str, SymbolKind)> = output1
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();

        assert!(names.contains(&("Base", SymbolKind::Class)));
        assert!(names.contains(&("constructor", SymbolKind::Method)));
        assert!(names.contains(&("method", SymbolKind::Method)));
        assert!(names.contains(&("Child", SymbolKind::Class)));
        assert!(names.contains(&("helper", SymbolKind::Method)));
        assert!(names.contains(&("free", SymbolKind::Function)));
        assert!(names.contains(&("asyncFn", SymbolKind::Function)));

        let method = find_symbol(&output1.symbols, "method");
        assert_eq!(method.qualified_name, "Base::method");
        assert_eq!(method.parent_qualified_name.as_deref(), Some("Base"));

        let base = find_symbol(&output1.symbols, "Base");
        assert!(base.docstring.is_some());
    }
}

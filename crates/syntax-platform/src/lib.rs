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
}

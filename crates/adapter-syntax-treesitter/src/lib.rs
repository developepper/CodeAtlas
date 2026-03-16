#![allow(deprecated)]

use adapter_api::{
    AdapterCapabilities, AdapterError, AdapterOutput, ExtractedSymbol, IndexContext,
    LanguageAdapter, SourceFile, SourceSpan,
};
use core_model::SymbolKind;
use tree_sitter::{Node, Parser};

mod languages;

use languages::LanguageProfile;

/// A tree-sitter-based syntax adapter that extracts symbols from source files.
///
/// Each adapter instance handles exactly one language. Use [`create_adapter`]
/// to obtain an adapter for a supported language.
pub struct TreeSitterAdapter {
    adapter_id: String,
    language_id: String,
    capabilities: AdapterCapabilities,
    profile: &'static LanguageProfile,
}

impl TreeSitterAdapter {
    /// Creates a new adapter for the given language profile.
    fn new(profile: &'static LanguageProfile) -> Self {
        Self {
            adapter_id: format!("syntax-treesitter-{}", profile.language_id),
            language_id: profile.language_id.to_string(),
            capabilities: AdapterCapabilities::syntax_baseline(),
            profile,
        }
    }
}

/// Create an adapter for the given language, if supported.
///
/// Returns `None` if no tree-sitter grammar is available for the language.
#[must_use]
pub fn create_adapter(language: &str) -> Option<TreeSitterAdapter> {
    languages::profile_for(language).map(TreeSitterAdapter::new)
}

/// Returns the list of languages supported by this crate.
#[must_use]
pub fn supported_languages() -> &'static [&'static str] {
    languages::SUPPORTED_LANGUAGES
}

impl LanguageAdapter for TreeSitterAdapter {
    fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    fn language(&self) -> &str {
        &self.language_id
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        &self.capabilities
    }

    fn index_file(
        &self,
        _ctx: &IndexContext,
        file: &SourceFile,
    ) -> Result<AdapterOutput, AdapterError> {
        if file.language != self.language_id {
            return Err(AdapterError::Unsupported {
                language: file.language.clone(),
            });
        }

        let mut parser = Parser::new();
        parser
            .set_language(&(self.profile.ts_language)())
            .map_err(|err| AdapterError::Parse {
                path: file.relative_path.clone(),
                reason: format!("failed to set language: {err}"),
            })?;

        let tree = parser
            .parse(&file.content, None)
            .ok_or_else(|| AdapterError::Parse {
                path: file.relative_path.clone(),
                reason: "tree-sitter parse returned no tree".to_string(),
            })?;

        let mut symbols = extract_symbols(tree.root_node(), &file.content, self.profile);

        // Resolve provenance: fill in default confidence for symbols that
        // did not receive a per-symbol override.
        let default_confidence = self.capabilities.default_confidence;
        for sym in &mut symbols {
            if sym.confidence_score.is_none() {
                sym.confidence_score = Some(default_confidence);
            }
        }

        Ok(AdapterOutput {
            symbols,
            source_adapter: self.adapter_id.clone(),
            quality_level: self.capabilities.quality_level,
        })
    }
}

// ---------------------------------------------------------------------------
// Symbol extraction
// ---------------------------------------------------------------------------

fn extract_symbols(root: Node, source: &[u8], profile: &LanguageProfile) -> Vec<ExtractedSymbol> {
    let mut symbols = Vec::new();
    let mut scope_stack: Vec<String> = Vec::new();
    walk_node(root, source, profile, &mut symbols, &mut scope_stack);
    symbols
}

fn walk_node(
    node: Node,
    source: &[u8],
    profile: &LanguageProfile,
    symbols: &mut Vec<ExtractedSymbol>,
    scope_stack: &mut Vec<String>,
) {
    let node_type = node.kind();

    // Check if this node is a symbol definition.
    if let Some(mapping) = profile.find_definition(node_type) {
        if let Some(name) = child_text(&node, mapping.name_field, source) {
            let kind = if mapping.kind == SymbolKind::Function && is_method_context(&node) {
                SymbolKind::Method
            } else {
                mapping.kind
            };

            let qualified_name = build_qualified_name(scope_stack, &name);
            let parent = if scope_stack.is_empty() {
                None
            } else {
                Some(scope_stack.join("::"))
            };

            symbols.push(ExtractedSymbol {
                name,
                qualified_name,
                kind,
                span: node_to_span(&node),
                signature: extract_signature(&node, source),
                confidence_score: None,
                docstring: extract_docstring(&node, source),
                parent_qualified_name: parent,
            });
        }
    }

    // Track scope for qualified names. Only pop if we actually pushed.
    let pushed_scope = if profile.is_scope_type(node_type) {
        if let Some(scope_name) = extract_scope_name(&node, source, profile) {
            scope_stack.push(scope_name);
            true
        } else {
            false
        }
    } else {
        false
    };

    // Recurse into children.
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_node(cursor.node(), source, profile, symbols, scope_stack);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if pushed_scope {
        scope_stack.pop();
    }
}

/// Returns `true` if a function node is inside an impl or trait body.
fn is_method_context(node: &Node) -> bool {
    node.parent()
        .and_then(|p| p.parent())
        .is_some_and(|gp| matches!(gp.kind(), "impl_item" | "trait_item"))
}

fn build_qualified_name(scope_stack: &[String], name: &str) -> String {
    if scope_stack.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope_stack.join("::"))
    }
}

fn node_to_span(node: &Node) -> SourceSpan {
    SourceSpan {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
        start_byte: node.start_byte() as u64,
        byte_length: (node.end_byte() - node.start_byte()) as u64,
    }
}

/// Extracts the text of a named child field.
fn child_text(node: &Node, field_name: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field_name)
        .and_then(|child| child.utf8_text(source).ok())
        .map(|s| s.to_string())
}

/// Extracts a signature by taking text from the node start up to the body.
fn extract_signature(node: &Node, source: &[u8]) -> String {
    // Find the body child (declaration_list, field_declaration_list, block, etc.)
    let body_start = find_body_start(node);

    let sig_end = body_start.unwrap_or(node.end_byte());
    let sig_bytes = &source[node.start_byte()..sig_end];
    let sig = String::from_utf8_lossy(sig_bytes);
    // Trim trailing whitespace and opening brace if present.
    sig.trim().trim_end_matches('{').trim().to_string()
}

fn find_body_start(node: &Node) -> Option<usize> {
    let body_fields = ["body", "block"];
    for field in &body_fields {
        if let Some(body) = node.child_by_field_name(field) {
            return Some(body.start_byte());
        }
    }
    // Fallback: look for a `{` child node.
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if cursor.node().kind() == "{" {
                return Some(cursor.node().start_byte());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Extracts doc comments preceding a definition node.
fn extract_docstring(node: &Node, source: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        match sib.kind() {
            "line_comment" => {
                let text = sib.utf8_text(source).unwrap_or("").trim();
                if let Some(doc) = text.strip_prefix("///") {
                    doc_lines.push(doc.strip_prefix(' ').unwrap_or(doc).to_string());
                } else {
                    break;
                }
            }
            "block_comment" => {
                let text = sib.utf8_text(source).unwrap_or("");
                if text.starts_with("/**") {
                    let inner = text
                        .strip_prefix("/**")
                        .and_then(|s| s.strip_suffix("*/"))
                        .unwrap_or(text)
                        .trim();
                    doc_lines.push(inner.to_string());
                }
                break;
            }
            _ => break,
        }
        sibling = sib.prev_sibling();
    }

    if doc_lines.is_empty() {
        return None;
    }
    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

/// Extracts the scope name from a scope-creating node.
fn extract_scope_name(node: &Node, source: &[u8], profile: &LanguageProfile) -> Option<String> {
    let node_type = node.kind();

    if let Some(scope_def) = profile.find_scope(node_type) {
        if let Some(type_node) = node.child_by_field_name(scope_def.name_field) {
            return extract_base_type_name(&type_node, source);
        }
    }

    None
}

/// Extracts the base type name, handling generic types like `Foo<T>`.
fn extract_base_type_name(node: &Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" | "identifier" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "generic_type" => {
            // For `Foo<T>`, extract just `Foo`.
            node.child_by_field_name("type")
                .and_then(|t| t.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        _ => node.utf8_text(source).ok().map(|s| s.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use adapter_api::LanguageAdapter;
    use std::path::PathBuf;

    fn index_rust_source(source: &str) -> AdapterOutput {
        let adapter = create_adapter("rust").expect("rust adapter");
        let ctx = IndexContext {
            repo_id: "test".to_string(),
            source_root: PathBuf::from("/tmp/test"),
        };
        let file = SourceFile {
            relative_path: PathBuf::from("src/lib.rs"),
            absolute_path: PathBuf::from("/tmp/test/src/lib.rs"),
            content: source.as_bytes().to_vec(),
            language: "rust".to_string(),
        };
        adapter.index_file(&ctx, &file).expect("index file")
    }

    fn find_symbol<'a>(symbols: &'a [ExtractedSymbol], name: &str) -> &'a ExtractedSymbol {
        symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{name}' not found in: {:?}", symbol_names(symbols)))
    }

    fn symbol_names(symbols: &[ExtractedSymbol]) -> Vec<&str> {
        symbols.iter().map(|s| s.name.as_str()).collect()
    }

    // -- Adapter identity --

    #[test]
    fn adapter_id_follows_naming_convention() {
        let adapter = create_adapter("rust").unwrap();
        assert_eq!(adapter.adapter_id(), "syntax-treesitter-rust");
        assert_eq!(adapter.language(), "rust");
    }

    #[test]
    fn adapter_capabilities_are_syntax_level() {
        let adapter = create_adapter("rust").unwrap();
        let caps = adapter.capabilities();
        assert_eq!(caps.quality_level, core_model::QualityLevel::Syntax);
        assert!(caps.default_confidence > 0.0 && caps.default_confidence <= 1.0);
    }

    #[test]
    fn unsupported_language_returns_none() {
        assert!(create_adapter("brainfuck").is_none());
    }

    #[test]
    fn language_mismatch_returns_error() {
        let adapter = create_adapter("rust").unwrap();
        let ctx = IndexContext {
            repo_id: "test".to_string(),
            source_root: PathBuf::from("/tmp"),
        };
        let file = SourceFile {
            relative_path: PathBuf::from("main.py"),
            absolute_path: PathBuf::from("/tmp/main.py"),
            content: b"print('hello')".to_vec(),
            language: "python".to_string(),
        };
        let err = adapter.index_file(&ctx, &file).expect_err("wrong language");
        assert!(err.to_string().contains("unsupported language"));
    }

    #[test]
    fn supported_languages_includes_rust() {
        assert!(supported_languages().contains(&"rust"));
    }

    // -- Provenance --

    #[test]
    fn output_carries_provenance_fields() {
        let output = index_rust_source("fn hello() {}\n");
        assert_eq!(output.source_adapter, "syntax-treesitter-rust");
        assert_eq!(output.quality_level, core_model::QualityLevel::Syntax);
    }

    #[test]
    fn symbols_have_resolved_confidence_scores() {
        let output = index_rust_source("fn hello() {}\nfn world() {}\n");
        for sym in &output.symbols {
            assert!(
                sym.confidence_score.is_some(),
                "symbol '{}' should have resolved confidence",
                sym.name
            );
            let score = sym.confidence_score.unwrap();
            assert!(score > 0.0 && score <= 1.0);
        }
    }

    // -- Function extraction --

    #[test]
    fn extracts_free_function() {
        let output = index_rust_source("fn hello() {}\n");
        assert_eq!(output.symbols.len(), 1);
        let sym = &output.symbols[0];
        assert_eq!(sym.name, "hello");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.qualified_name, "hello");
        assert_eq!(sym.span.start_line, 1);
        assert!(sym.span.byte_length > 0);
        assert!(sym.parent_qualified_name.is_none());
    }

    #[test]
    fn extracts_function_signature() {
        let output = index_rust_source("pub fn process(input: &str) -> bool {\n    true\n}\n");
        let sym = find_symbol(&output.symbols, "process");
        assert_eq!(sym.signature, "pub fn process(input: &str) -> bool");
    }

    // -- Struct extraction --

    #[test]
    fn extracts_struct_as_class() {
        let output = index_rust_source("struct Point {\n    x: f64,\n    y: f64,\n}\n");
        let sym = find_symbol(&output.symbols, "Point");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.qualified_name, "Point");
    }

    // -- Impl methods --

    #[test]
    fn extracts_impl_methods_with_qualified_names() {
        let source = "struct Foo;\nimpl Foo {\n    fn bar() {}\n    fn baz() {}\n}\n";
        let output = index_rust_source(source);
        let bar = find_symbol(&output.symbols, "bar");
        assert_eq!(bar.kind, SymbolKind::Method);
        assert_eq!(bar.qualified_name, "Foo::bar");
        assert_eq!(bar.parent_qualified_name.as_deref(), Some("Foo"));
    }

    #[test]
    fn extracts_impl_methods_for_generic_type() {
        let source = "struct Wrapper<T>(T);\nimpl<T> Wrapper<T> {\n    fn inner(&self) -> &T { &self.0 }\n}\n";
        let output = index_rust_source(source);
        let sym = find_symbol(&output.symbols, "inner");
        assert_eq!(sym.kind, SymbolKind::Method);
        assert_eq!(sym.qualified_name, "Wrapper::inner");
    }

    // -- Enum, trait, const, type alias --

    #[test]
    fn extracts_enum_as_type() {
        let output = index_rust_source("enum Color {\n    Red,\n    Green,\n}\n");
        let sym = find_symbol(&output.symbols, "Color");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn extracts_trait_as_type() {
        let output = index_rust_source("trait Drawable {\n    fn draw(&self);\n}\n");
        let sym = find_symbol(&output.symbols, "Drawable");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    #[test]
    fn extracts_trait_method_declarations() {
        let source = "trait Drawable {\n    fn draw(&self);\n}\n";
        let output = index_rust_source(source);
        let draw = find_symbol(&output.symbols, "draw");
        assert_eq!(draw.kind, SymbolKind::Method);
        assert_eq!(draw.qualified_name, "Drawable::draw");
    }

    #[test]
    fn extracts_const() {
        let output = index_rust_source("const MAX_SIZE: usize = 100;\n");
        let sym = find_symbol(&output.symbols, "MAX_SIZE");
        assert_eq!(sym.kind, SymbolKind::Constant);
    }

    #[test]
    fn extracts_static() {
        let output = index_rust_source("static INSTANCE: u32 = 0;\n");
        let sym = find_symbol(&output.symbols, "INSTANCE");
        assert_eq!(sym.kind, SymbolKind::Constant);
    }

    #[test]
    fn extracts_type_alias() {
        let output = index_rust_source("type Result<T> = std::result::Result<T, Error>;\n");
        let sym = find_symbol(&output.symbols, "Result");
        assert_eq!(sym.kind, SymbolKind::Type);
    }

    // -- Docstrings --

    #[test]
    fn extracts_doc_comments() {
        let source = "/// Does something useful.\n/// With multiple lines.\nfn documented() {}\n";
        let output = index_rust_source(source);
        let sym = find_symbol(&output.symbols, "documented");
        assert_eq!(
            sym.docstring.as_deref(),
            Some("Does something useful.\nWith multiple lines.")
        );
    }

    #[test]
    fn no_docstring_when_absent() {
        let output = index_rust_source("fn bare() {}\n");
        let sym = find_symbol(&output.symbols, "bare");
        assert!(sym.docstring.is_none());
    }

    // -- Edge cases --

    #[test]
    fn empty_file_produces_no_symbols() {
        let output = index_rust_source("");
        assert!(output.symbols.is_empty());
    }

    #[test]
    fn whitespace_only_file_produces_no_symbols() {
        let output = index_rust_source("   \n\n  \n");
        assert!(output.symbols.is_empty());
    }

    // -- Comprehensive fixture --

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

        let output1 = index_rust_source(source);
        let output2 = index_rust_source(source);

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

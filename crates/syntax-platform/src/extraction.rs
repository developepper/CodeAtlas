//! Shared tree-sitter extraction utilities.
//!
//! These functions are used by all language modules to walk a tree-sitter
//! parse tree and extract symbols according to a [`LanguageProfile`].

use core_model::{SourceSpan, SymbolKind};
use tree_sitter::Node;

use crate::languages::LanguageProfile;
use crate::types::SyntaxSymbol;

/// Extract symbols from a parsed tree-sitter tree using the given language profile.
pub fn extract_symbols(root: Node, source: &[u8], profile: &LanguageProfile) -> Vec<SyntaxSymbol> {
    let mut symbols = Vec::new();
    let mut scope_stack: Vec<String> = Vec::new();
    walk_node(root, source, profile, &mut symbols, &mut scope_stack);
    symbols
}

fn walk_node(
    node: Node,
    source: &[u8],
    profile: &LanguageProfile,
    symbols: &mut Vec<SyntaxSymbol>,
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

            symbols.push(SyntaxSymbol {
                name,
                qualified_name,
                kind,
                span: node_to_span(&node),
                signature: extract_signature(&node, source),
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

/// Converts a tree-sitter node position to a [`SourceSpan`].
pub fn node_to_span(node: &Node) -> SourceSpan {
    SourceSpan {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
        start_byte: node.start_byte() as u64,
        byte_length: (node.end_byte() - node.start_byte()) as u64,
    }
}

/// Extracts the text of a named child field.
pub fn child_text(node: &Node, field_name: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field_name)
        .and_then(|child| child.utf8_text(source).ok())
        .map(|s| s.to_string())
}

/// Extracts a signature by taking text from the node start up to the body.
pub fn extract_signature(node: &Node, source: &[u8]) -> String {
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
pub fn extract_docstring(node: &Node, source: &[u8]) -> Option<String> {
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

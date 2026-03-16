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
            // Check modifier requirements (e.g. Java `static final` fields).
            if !mapping.requires_modifiers.is_empty()
                && !check_ancestor_modifiers(&node, mapping.requires_modifiers, source)
            {
                // Modifiers not satisfied — skip this node but continue walking children.
            } else if !mapping.requires_value_types.is_empty()
                && !check_value_type(&node, mapping.requires_value_types)
            {
                // Value type not satisfied — skip (e.g. `const x = 3` is not a function).
            } else {
                let kind = if mapping.kind == SymbolKind::Function && is_method_context(&node) {
                    SymbolKind::Method
                } else {
                    mapping.kind
                };

                // Check for receiver-based parent (e.g. Go methods).
                let receiver_type = profile
                    .find_method_receiver(node_type)
                    .and_then(|rm| extract_receiver_type(&node, rm.receiver_field, source));

                let (qualified_name, parent) = if let Some(ref recv) = receiver_type {
                    let qn = format!("{recv}::{name}");
                    (qn, Some(recv.clone()))
                } else {
                    let qn = build_qualified_name(scope_stack, &name);
                    let p = if scope_stack.is_empty() {
                        None
                    } else {
                        Some(scope_stack.join("::"))
                    };
                    (qn, p)
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
            } // else (modifier check)
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

    // Recurse into children, tracking sticky scopes across siblings.
    let mut cursor = node.walk();
    let mut sticky_pushed = false;
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // Check for sticky scope (e.g. PHP statement-form namespace).
            // A sticky scope applies to all subsequent siblings. When we
            // encounter a new one, replace the previous sticky scope.
            if let Some(sticky) = profile.find_sticky_scope(child.kind()) {
                if let Some(name) = child_text(&child, sticky.name_field, source)
                    .map(|s| normalize_scope_separator(&s))
                {
                    // Only treat as sticky when the node has no body
                    // (statement form). Braced namespaces with a body are
                    // handled as regular scopes via the child-scope push below.
                    let has_body = child.child_by_field_name("body").is_some();
                    if !has_body {
                        if sticky_pushed {
                            scope_stack.pop();
                        }
                        scope_stack.push(name);
                        sticky_pushed = true;

                        // Skip recursing into the namespace_definition itself
                        // — it contains no extractable symbols.
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                        continue;
                    }
                }
            }

            walk_node(child, source, profile, symbols, scope_stack);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Pop sticky scope if one was active at this level.
    if sticky_pushed {
        scope_stack.pop();
    }

    if pushed_scope {
        scope_stack.pop();
    }
}

/// Checks whether a node's `value` child matches one of the required types.
///
/// Used to filter JS `variable_declarator` nodes: only extract when the value
/// is an `arrow_function` or `function_expression`, not plain data.
fn check_value_type(node: &Node, required_types: &[&str]) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|v| required_types.contains(&v.kind()))
}

/// Checks whether a node or its parent has a `modifiers` child containing
/// all required keywords (e.g. `["static", "final"]` for Java constants).
fn check_ancestor_modifiers(node: &Node, required: &[&str], source: &[u8]) -> bool {
    // Check the node itself and its parent for a `modifiers` child.
    for n in [Some(*node), node.parent()].into_iter().flatten() {
        if let Some(mods) = n.child_by_field_name("modifiers") {
            if let Ok(text) = mods.utf8_text(source) {
                if required.iter().all(|kw| text.contains(kw)) {
                    return true;
                }
            }
        }
        // Also check unnamed children of type "modifiers".
        let mut cursor = n.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "modifiers" {
                    if let Ok(text) = child.utf8_text(source) {
                        if required.iter().all(|kw| text.contains(kw)) {
                            return true;
                        }
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    false
}

/// Returns `true` if a function node is inside a class, impl, or trait body.
///
/// Checks up to three ancestor levels to handle decorated functions:
/// - Rust:   `function_item` → `declaration_list` → `impl_item`
/// - Python: `function_definition` → `block` → `class_definition`
/// - Python decorated: `function_definition` → `decorated_definition` → `block` → `class_definition`
fn is_method_context(node: &Node) -> bool {
    let class_like = |kind: &str| {
        matches!(
            kind,
            "impl_item" | "trait_item" | "class_definition" | "class_declaration"
        )
    };

    // Check grandparent (normal case).
    if let Some(gp) = node.parent().and_then(|p| p.parent()) {
        if class_like(gp.kind()) {
            return true;
        }
        // Check great-grandparent (decorated case in Python).
        if let Some(ggp) = gp.parent() {
            if class_like(ggp.kind()) {
                return true;
            }
        }
    }
    false
}

/// Extracts the receiver type name from a Go-style method receiver parameter.
///
/// Given a `method_declaration` node with a `receiver` field like
/// `(c *Config)` or `(c Config)`, returns `"Config"`.
fn extract_receiver_type(node: &Node, receiver_field: &str, source: &[u8]) -> Option<String> {
    let receiver = node.child_by_field_name(receiver_field)?;

    // The receiver is a `parameter_list` containing one `parameter_declaration`.
    let param = receiver.named_child(0)?;
    if param.kind() != "parameter_declaration" {
        return None;
    }

    // The type field may be `pointer_type` (e.g. `*Config`) or a direct type
    // identifier (e.g. `Config`).
    let type_node = param.child_by_field_name("type")?;
    match type_node.kind() {
        "pointer_type" => {
            // `*Config` — the inner type_identifier is the first named child.
            type_node
                .named_child(0)
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        _ => type_node.utf8_text(source).ok().map(|s| s.to_string()),
    }
}

/// Replaces language-specific namespace separators with `::`.
///
/// Handles PHP `\` and Java `.` separators.
fn normalize_scope_separator(raw: &str) -> String {
    raw.replace(['\\', '.'], "::")
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
///
/// First tries `child_by_field_name` (tree-sitter field). If that returns
/// nothing, falls back to finding the first child whose node *type* matches
/// `field_name`. This fallback handles grammars like PHP where some names
/// are positional children of type `name` rather than named fields.
pub fn child_text(node: &Node, field_name: &str, source: &[u8]) -> Option<String> {
    // Primary: named field lookup.
    if let Some(text) = node
        .child_by_field_name(field_name)
        .and_then(|child| child.utf8_text(source).ok())
        .map(|s| s.to_string())
    {
        return Some(text);
    }

    // Fallback: first child whose node type matches.
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == field_name {
                return child.utf8_text(source).ok().map(|s| s.to_string());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
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
///
/// Checks the node's own preceding siblings first. If none found, also checks
/// the parent node's preceding siblings. This handles Go's `type_spec` inside
/// `type_declaration` where the doc comment precedes the outer declaration.
pub fn extract_docstring(node: &Node, source: &[u8]) -> Option<String> {
    if let Some(doc) = extract_preceding_doc(node, source) {
        return Some(doc);
    }
    // Fallback: check parent's preceding siblings (e.g. type_spec → type_declaration).
    if let Some(parent) = node.parent() {
        if let Some(doc) = extract_preceding_doc(&parent, source) {
            return Some(doc);
        }
    }
    // Fallback: Python-style body docstrings.
    extract_body_docstring(node, source)
}

fn extract_preceding_doc(node: &Node, source: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        match sib.kind() {
            "line_comment" => {
                let text = sib.utf8_text(source).unwrap_or("").trim();
                // Rust `///` doc comments.
                if let Some(doc) = text.strip_prefix("///") {
                    doc_lines.push(doc.strip_prefix(' ').unwrap_or(doc).to_string());
                } else {
                    break;
                }
            }
            // Go/C-style `// comment` (tree-sitter uses `comment` node type).
            "comment" if !sib.utf8_text(source).unwrap_or("").starts_with("/*") => {
                let text = sib.utf8_text(source).unwrap_or("").trim();
                if let Some(doc) = text.strip_prefix("//") {
                    doc_lines.push(doc.strip_prefix(' ').unwrap_or(doc).to_string());
                } else {
                    break;
                }
            }
            // Rust: `block_comment`, PHP/others: `comment`
            "block_comment" | "comment" => {
                let text = sib.utf8_text(source).unwrap_or("");
                if text.starts_with("/**") {
                    let cleaned = clean_block_doc_comment(text);
                    if !cleaned.is_empty() {
                        doc_lines.push(cleaned);
                    }
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

/// Extracts a Python-style docstring from the first statement in a body block.
///
/// Python docstrings are string literals appearing as the first expression
/// statement inside a function or class body:
///
/// ```python
/// def foo():
///     """This is the docstring."""
///     pass
/// ```
fn extract_body_docstring(node: &Node, source: &[u8]) -> Option<String> {
    let body = node.child_by_field_name("body")?;

    // Find the first named child of the body block.
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    // Skip non-named nodes.
    while !cursor.node().is_named() {
        if !cursor.goto_next_sibling() {
            return None;
        }
    }

    let first = cursor.node();
    if first.kind() != "expression_statement" {
        return None;
    }

    // The expression_statement should contain a single string child.
    let string_node = first.named_child(0)?;
    if string_node.kind() != "string" {
        return None;
    }

    let raw = string_node.utf8_text(source).ok()?;

    // Strip triple-quote delimiters.
    let inner = raw
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
        .or_else(|| raw.strip_prefix("'''").and_then(|s| s.strip_suffix("'''")))
        .unwrap_or(raw);

    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Cleans a `/** ... */` doc comment, stripping delimiters and leading `*`.
fn clean_block_doc_comment(text: &str) -> String {
    let inner = text
        .strip_prefix("/**")
        .and_then(|s| s.strip_suffix("*/"))
        .unwrap_or(text);

    // For multi-line PHPDoc/Javadoc, strip leading ` * ` from each line.
    let lines: Vec<&str> = inner.lines().collect();
    if lines.len() <= 1 {
        return inner.trim().to_string();
    }

    let cleaned: Vec<String> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("* ")
                .or_else(|| trimmed.strip_prefix('*'))
                .unwrap_or(trimmed)
                .to_string()
        })
        .collect();

    // Drop empty leading/trailing lines from the stripped block.
    let start = cleaned.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = cleaned
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    cleaned[start..end].join("\n")
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

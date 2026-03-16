use core_model::SymbolKind;

use super::{LanguageProfile, NodeMapping, ScopeMapping};

fn javascript_language() -> tree_sitter::Language {
    tree_sitter_javascript::LANGUAGE.into()
}

static JS_DEFINITIONS: &[NodeMapping] = &[
    NodeMapping {
        node_type: "function_declaration",
        kind: SymbolKind::Function,
        name_field: "name",
        requires_modifiers: &[],
        requires_value_types: &[],
    },
    // JS `method_definition` covers regular methods, constructors,
    // getters, setters, and static methods inside class bodies.
    NodeMapping {
        node_type: "method_definition",
        kind: SymbolKind::Method,
        name_field: "name",
        requires_modifiers: &[],
        requires_value_types: &[],
    },
    NodeMapping {
        node_type: "class_declaration",
        kind: SymbolKind::Class,
        name_field: "name",
        requires_modifiers: &[],
        requires_value_types: &[],
    },
    // Arrow functions and function expressions assigned to variables:
    //   const createApp = (config) => ({ config });
    //   const helper = function(x) { return x + 1; };
    // The `requires_value_types` filter ensures we only extract these when
    // the value is a function-like node, not plain data like `const x = 3`.
    NodeMapping {
        node_type: "variable_declarator",
        kind: SymbolKind::Function,
        name_field: "name",
        requires_modifiers: &[],
        requires_value_types: &["arrow_function", "function_expression"],
    },
];

static JS_SCOPES: &[ScopeMapping] = &[ScopeMapping {
    node_type: "class_declaration",
    name_field: "name",
}];

pub static JAVASCRIPT_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "javascript",
    ts_language: javascript_language,
    definitions: JS_DEFINITIONS,
    scopes: JS_SCOPES,
    sticky_scopes: &[],
    method_receivers: &[],
};

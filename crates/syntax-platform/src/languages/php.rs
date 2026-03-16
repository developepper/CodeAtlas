use core_model::SymbolKind;

use super::{LanguageProfile, NodeMapping, ScopeMapping, StickyScopeMapping};

fn php_language() -> tree_sitter::Language {
    tree_sitter_php::LANGUAGE_PHP.into()
}

static PHP_DEFINITIONS: &[NodeMapping] = &[
    NodeMapping {
        node_type: "function_definition",
        kind: SymbolKind::Function,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "method_declaration",
        kind: SymbolKind::Method,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "class_declaration",
        kind: SymbolKind::Class,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "interface_declaration",
        kind: SymbolKind::Type,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "trait_declaration",
        kind: SymbolKind::Type,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "enum_declaration",
        kind: SymbolKind::Type,
        name_field: "name",
        requires_modifiers: &[],
    },
    // Constants: the name lives on `const_element` as a child of type
    // `name` (not a named field). The `child_text` fallback handles this.
    NodeMapping {
        node_type: "const_element",
        kind: SymbolKind::Constant,
        name_field: "name",
        requires_modifiers: &[],
    },
];

static PHP_SCOPES: &[ScopeMapping] = &[
    ScopeMapping {
        node_type: "class_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "interface_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "trait_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "enum_declaration",
        name_field: "name",
    },
    // Braced namespace form: `namespace Foo { class Bar {} }`
    // The class is a child of the namespace body, so normal scope works.
    ScopeMapping {
        node_type: "namespace_definition",
        name_field: "name",
    },
];

/// Statement-form namespace: `namespace App\Models;`
///
/// The namespace node is a sibling of the class declarations it qualifies,
/// not their parent. It applies to all subsequent siblings until the next
/// namespace declaration or end of the enclosing scope.
static PHP_STICKY_SCOPES: &[StickyScopeMapping] = &[StickyScopeMapping {
    node_type: "namespace_definition",
    name_field: "name",
}];

pub static PHP_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "php",
    ts_language: php_language,
    definitions: PHP_DEFINITIONS,
    scopes: PHP_SCOPES,
    sticky_scopes: PHP_STICKY_SCOPES,
    method_receivers: &[],
};

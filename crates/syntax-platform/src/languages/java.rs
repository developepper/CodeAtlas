use core_model::SymbolKind;

use super::{LanguageProfile, NodeMapping, ScopeMapping, StickyScopeMapping};

fn java_language() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

static JAVA_DEFINITIONS: &[NodeMapping] = &[
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
        node_type: "enum_declaration",
        kind: SymbolKind::Type,
        name_field: "name",
        requires_modifiers: &[],
    },
    // Java records are value-based classes.
    NodeMapping {
        node_type: "record_declaration",
        kind: SymbolKind::Class,
        name_field: "name",
        requires_modifiers: &[],
    },
    // Methods are always inside a class/interface/enum body.
    NodeMapping {
        node_type: "method_declaration",
        kind: SymbolKind::Method,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "constructor_declaration",
        kind: SymbolKind::Method,
        name_field: "name",
        requires_modifiers: &[],
    },
    NodeMapping {
        node_type: "enum_constant",
        kind: SymbolKind::Constant,
        name_field: "name",
        requires_modifiers: &[],
    },
    // Java `static final` field constants. The `variable_declarator` node
    // carries the name; the `requires_modifiers` filter ensures we only
    // extract fields that are declared `static final`, not regular fields.
    NodeMapping {
        node_type: "variable_declarator",
        kind: SymbolKind::Constant,
        name_field: "name",
        requires_modifiers: &["static", "final"],
    },
];

static JAVA_SCOPES: &[ScopeMapping] = &[
    ScopeMapping {
        node_type: "class_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "interface_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "enum_declaration",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "record_declaration",
        name_field: "name",
    },
];

/// Java `package` declaration (`package com.example.app;`).
///
/// The package name is a `scoped_identifier` positional child (not a named
/// field). The `child_text` fallback in the sticky scope handler resolves
/// this by finding the first child of type `scoped_identifier`.
static JAVA_STICKY_SCOPES: &[StickyScopeMapping] = &[StickyScopeMapping {
    node_type: "package_declaration",
    name_field: "scoped_identifier",
}];

pub static JAVA_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "java",
    ts_language: java_language,
    definitions: JAVA_DEFINITIONS,
    scopes: JAVA_SCOPES,
    sticky_scopes: JAVA_STICKY_SCOPES,
    method_receivers: &[],
};

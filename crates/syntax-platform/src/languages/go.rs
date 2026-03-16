use core_model::SymbolKind;

use super::{LanguageProfile, MethodReceiverMapping, NodeMapping, ScopeMapping};

fn go_language() -> tree_sitter::Language {
    tree_sitter_go::LANGUAGE.into()
}

static GO_DEFINITIONS: &[NodeMapping] = &[
    NodeMapping {
        node_type: "function_declaration",
        kind: SymbolKind::Function,
        name_field: "name",
        requires_modifiers: &[],
    },
    // Go has a dedicated `method_declaration` node with a receiver.
    // Mapped directly to Method; receiver-based parent is handled by
    // the extraction engine via `method_receivers`.
    NodeMapping {
        node_type: "method_declaration",
        kind: SymbolKind::Method,
        name_field: "name",
        requires_modifiers: &[],
    },
    // `type_spec` is the inner node of `type_declaration`. It carries
    // the name and the underlying type (struct, interface, alias, etc.).
    NodeMapping {
        node_type: "type_spec",
        kind: SymbolKind::Type,
        name_field: "name",
        requires_modifiers: &[],
    },
    // `const_spec` is the inner node of `const_declaration`.
    NodeMapping {
        node_type: "const_spec",
        kind: SymbolKind::Constant,
        name_field: "name",
        requires_modifiers: &[],
    },
];

// Go has no class-like scopes — all declarations are package-level.
static GO_SCOPES: &[ScopeMapping] = &[];

static GO_METHOD_RECEIVERS: &[MethodReceiverMapping] = &[MethodReceiverMapping {
    method_node_type: "method_declaration",
    receiver_field: "receiver",
}];

pub static GO_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "go",
    ts_language: go_language,
    definitions: GO_DEFINITIONS,
    scopes: GO_SCOPES,
    sticky_scopes: &[],
    method_receivers: GO_METHOD_RECEIVERS,
};

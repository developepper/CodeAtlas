use core_model::SymbolKind;

use super::{LanguageProfile, NodeMapping, ScopeMapping};

fn rust_language() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

static RUST_DEFINITIONS: &[NodeMapping] = &[
    NodeMapping {
        node_type: "function_item",
        kind: SymbolKind::Function,
        name_field: "name",
    },
    NodeMapping {
        node_type: "function_signature_item",
        kind: SymbolKind::Function,
        name_field: "name",
    },
    NodeMapping {
        node_type: "struct_item",
        kind: SymbolKind::Class,
        name_field: "name",
    },
    NodeMapping {
        node_type: "enum_item",
        kind: SymbolKind::Type,
        name_field: "name",
    },
    NodeMapping {
        node_type: "trait_item",
        kind: SymbolKind::Type,
        name_field: "name",
    },
    NodeMapping {
        node_type: "type_item",
        kind: SymbolKind::Type,
        name_field: "name",
    },
    NodeMapping {
        node_type: "const_item",
        kind: SymbolKind::Constant,
        name_field: "name",
    },
    NodeMapping {
        node_type: "static_item",
        kind: SymbolKind::Constant,
        name_field: "name",
    },
];

static RUST_SCOPES: &[ScopeMapping] = &[
    ScopeMapping {
        node_type: "impl_item",
        name_field: "type",
    },
    ScopeMapping {
        node_type: "trait_item",
        name_field: "name",
    },
    ScopeMapping {
        node_type: "mod_item",
        name_field: "name",
    },
];

pub static RUST_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "rust",
    ts_language: rust_language,
    definitions: RUST_DEFINITIONS,
    scopes: RUST_SCOPES,
    sticky_scopes: &[],
};

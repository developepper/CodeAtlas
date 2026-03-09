use core_model::SymbolKind;

/// Languages supported by this crate.
pub static SUPPORTED_LANGUAGES: &[&str] = &["rust"];

/// A mapping from a tree-sitter node type to a symbol kind.
pub struct NodeMapping {
    pub node_type: &'static str,
    pub kind: SymbolKind,
    pub name_field: &'static str,
}

/// Describes how to extract scope names from scope-creating nodes.
pub struct ScopeMapping {
    pub node_type: &'static str,
    pub name_field: &'static str,
}

/// Language-specific configuration for tree-sitter symbol extraction.
pub struct LanguageProfile {
    pub language_id: &'static str,
    pub ts_language: fn() -> tree_sitter::Language,
    definitions: &'static [NodeMapping],
    scopes: &'static [ScopeMapping],
}

impl LanguageProfile {
    pub fn find_definition(&self, node_type: &str) -> Option<&NodeMapping> {
        self.definitions.iter().find(|m| m.node_type == node_type)
    }

    pub fn is_scope_type(&self, node_type: &str) -> bool {
        self.scopes.iter().any(|s| s.node_type == node_type)
    }

    pub fn find_scope(&self, node_type: &str) -> Option<&ScopeMapping> {
        self.scopes.iter().find(|s| s.node_type == node_type)
    }
}

/// Returns the language profile for the given language ID, if supported.
pub fn profile_for(language: &str) -> Option<&'static LanguageProfile> {
    match language {
        "rust" => Some(&RUST_PROFILE),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

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

static RUST_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "rust",
    ts_language: rust_language,
    definitions: RUST_DEFINITIONS,
    scopes: RUST_SCOPES,
};

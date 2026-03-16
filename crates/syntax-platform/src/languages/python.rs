use core_model::SymbolKind;

use super::{LanguageProfile, NodeMapping, ScopeMapping};

fn python_language() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

static PYTHON_DEFINITIONS: &[NodeMapping] = &[
    // Python uses `function_definition` for both free functions and methods.
    // The extraction engine promotes to `Method` when inside a class body.
    NodeMapping {
        node_type: "function_definition",
        kind: SymbolKind::Function,
        name_field: "name",
    },
    NodeMapping {
        node_type: "class_definition",
        kind: SymbolKind::Class,
        name_field: "name",
    },
];

static PYTHON_SCOPES: &[ScopeMapping] = &[ScopeMapping {
    node_type: "class_definition",
    name_field: "name",
}];

pub static PYTHON_PROFILE: LanguageProfile = LanguageProfile {
    language_id: "python",
    ts_language: python_language,
    definitions: PYTHON_DEFINITIONS,
    scopes: PYTHON_SCOPES,
    sticky_scopes: &[],
};

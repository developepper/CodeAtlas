pub mod php;
pub mod python;
pub mod rust;

use core_model::SymbolKind;

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

/// A scope that applies to all *subsequent siblings* rather than children.
///
/// PHP's statement-form namespace (`namespace App\Models;`) is the primary
/// example: it is a sibling of the class declarations it qualifies, not their
/// parent in the parse tree.
pub struct StickyScopeMapping {
    pub node_type: &'static str,
    pub name_field: &'static str,
}

/// Language-specific configuration for tree-sitter symbol extraction.
pub struct LanguageProfile {
    pub language_id: &'static str,
    pub ts_language: fn() -> tree_sitter::Language,
    definitions: &'static [NodeMapping],
    scopes: &'static [ScopeMapping],
    /// Scopes that persist across subsequent siblings (e.g. PHP namespaces).
    sticky_scopes: &'static [StickyScopeMapping],
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

    pub fn find_sticky_scope(&self, node_type: &str) -> Option<&StickyScopeMapping> {
        self.sticky_scopes.iter().find(|s| s.node_type == node_type)
    }
}

/// Returns the language profile for the given language ID, if supported.
pub fn profile_for(language: &str) -> Option<&'static LanguageProfile> {
    match language {
        "php" => Some(&php::PHP_PROFILE),
        "python" => Some(&python::PYTHON_PROFILE),
        "rust" => Some(&rust::RUST_PROFILE),
        _ => None,
    }
}

/// Returns the list of languages with syntax backends on this platform.
pub fn supported_languages() -> &'static [&'static str] {
    &["php", "python", "rust"]
}

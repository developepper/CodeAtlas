pub mod go;
pub mod java;
pub mod javascript;
pub mod php;
pub mod python;
pub mod rust;

use core_model::SymbolKind;

/// A mapping from a tree-sitter node type to a symbol kind.
pub struct NodeMapping {
    pub node_type: &'static str,
    pub kind: SymbolKind,
    pub name_field: &'static str,
    /// When non-empty, only match if the parent node has a `modifiers` child
    /// containing ALL of these keywords. Used to extract Java `static final`
    /// field declarations as constants while ignoring regular fields.
    pub requires_modifiers: &'static [&'static str],
    /// When non-empty, only match if the node's `value` child is one of these
    /// node types. Used to extract JS arrow functions and function expressions
    /// assigned to variables while skipping plain data assignments.
    pub requires_value_types: &'static [&'static str],
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

/// Describes how to extract the receiver type from a method declaration.
///
/// Go methods have a receiver parameter (`func (c *Config) GetName()`) that
/// serves as the parent/container for qualified name construction. The method
/// is a top-level declaration, not nested inside the struct.
pub struct MethodReceiverMapping {
    pub method_node_type: &'static str,
    pub receiver_field: &'static str,
}

/// Language-specific configuration for tree-sitter symbol extraction.
pub struct LanguageProfile {
    pub language_id: &'static str,
    pub ts_language: fn() -> tree_sitter::Language,
    definitions: &'static [NodeMapping],
    scopes: &'static [ScopeMapping],
    /// Scopes that persist across subsequent siblings (e.g. PHP namespaces).
    sticky_scopes: &'static [StickyScopeMapping],
    /// Method receiver mappings (e.g. Go receiver parameters).
    method_receivers: &'static [MethodReceiverMapping],
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

    pub fn find_method_receiver(&self, node_type: &str) -> Option<&MethodReceiverMapping> {
        self.method_receivers
            .iter()
            .find(|m| m.method_node_type == node_type)
    }
}

/// Returns the language profile for the given language ID, if supported.
pub fn profile_for(language: &str) -> Option<&'static LanguageProfile> {
    match language {
        "go" => Some(&go::GO_PROFILE),
        "java" => Some(&java::JAVA_PROFILE),
        "javascript" => Some(&javascript::JAVASCRIPT_PROFILE),
        "php" => Some(&php::PHP_PROFILE),
        "python" => Some(&python::PYTHON_PROFILE),
        "rust" => Some(&rust::RUST_PROFILE),
        _ => None,
    }
}

/// Returns the list of languages with syntax backends on this platform.
pub fn supported_languages() -> &'static [&'static str] {
    &["go", "java", "javascript", "php", "python", "rust"]
}

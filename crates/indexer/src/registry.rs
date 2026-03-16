//! Backend registry: stores all syntax and semantic backends by language.

use std::collections::HashMap;

use core_model::BackendId;
use semantic_api::SemanticBackend;
use syntax_platform::SyntaxBackend;

/// Registry of all available syntax and semantic backends.
///
/// Backends are registered at pipeline startup. The registry is immutable
/// after construction.
pub trait BackendRegistry: Send + Sync {
    /// Returns IDs of all syntax backends registered for a language.
    fn syntax_backends(&self, language: &str) -> Vec<BackendId>;

    /// Returns IDs of all semantic backends registered for a language.
    fn semantic_backends(&self, language: &str) -> Vec<BackendId>;

    /// Returns a reference to the syntax backend with the given ID.
    /// Panics if the ID is not registered.
    fn syntax(&self, id: &BackendId) -> &dyn SyntaxBackend;

    /// Returns a reference to the semantic backend with the given ID.
    /// Panics if the ID is not registered.
    fn semantic(&self, id: &BackendId) -> &dyn SemanticBackend;

    /// Returns all languages that have at least one registered syntax backend.
    fn syntax_languages(&self) -> Vec<&str>;

    /// Returns all languages that have at least one registered semantic backend.
    fn semantic_languages(&self) -> Vec<&str>;
}

/// Default production implementation of [`BackendRegistry`].
pub struct DefaultBackendRegistry {
    syntax_map: HashMap<BackendId, Box<dyn SyntaxBackend>>,
    semantic_map: HashMap<BackendId, Box<dyn SemanticBackend>>,
    syntax_by_lang: HashMap<String, Vec<BackendId>>,
    semantic_by_lang: HashMap<String, Vec<BackendId>>,
}

impl DefaultBackendRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            syntax_map: HashMap::new(),
            semantic_map: HashMap::new(),
            syntax_by_lang: HashMap::new(),
            semantic_by_lang: HashMap::new(),
        }
    }

    /// Register a syntax backend.
    pub fn register_syntax(&mut self, id: BackendId, backend: Box<dyn SyntaxBackend>) {
        let lang = backend.language().to_string();
        self.syntax_by_lang
            .entry(lang)
            .or_default()
            .push(id.clone());
        self.syntax_map.insert(id, backend);
    }

    /// Register a semantic backend.
    pub fn register_semantic(&mut self, id: BackendId, backend: Box<dyn SemanticBackend>) {
        let lang = backend.language().to_string();
        self.semantic_by_lang
            .entry(lang)
            .or_default()
            .push(id.clone());
        self.semantic_map.insert(id, backend);
    }

    /// Returns all registered backend IDs (for diagnostics / logging).
    #[must_use]
    pub fn all_backend_ids(&self) -> Vec<&BackendId> {
        let mut ids: Vec<&BackendId> = self.syntax_map.keys().collect();
        ids.extend(self.semantic_map.keys());
        ids
    }
}

impl Default for DefaultBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendRegistry for DefaultBackendRegistry {
    fn syntax_backends(&self, language: &str) -> Vec<BackendId> {
        self.syntax_by_lang
            .get(language)
            .cloned()
            .unwrap_or_default()
    }

    fn semantic_backends(&self, language: &str) -> Vec<BackendId> {
        self.semantic_by_lang
            .get(language)
            .cloned()
            .unwrap_or_default()
    }

    fn syntax(&self, id: &BackendId) -> &dyn SyntaxBackend {
        self.syntax_map
            .get(id)
            .unwrap_or_else(|| panic!("syntax backend not registered: {id}"))
            .as_ref()
    }

    fn semantic(&self, id: &BackendId) -> &dyn SemanticBackend {
        self.semantic_map
            .get(id)
            .unwrap_or_else(|| panic!("semantic backend not registered: {id}"))
            .as_ref()
    }

    fn syntax_languages(&self) -> Vec<&str> {
        self.syntax_by_lang.keys().map(|s| s.as_str()).collect()
    }

    fn semantic_languages(&self) -> Vec<&str> {
        self.semantic_by_lang.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_returns_empty() {
        let reg = DefaultBackendRegistry::new();
        assert!(reg.syntax_backends("rust").is_empty());
        assert!(reg.semantic_backends("rust").is_empty());
        assert!(reg.syntax_languages().is_empty());
        assert!(reg.semantic_languages().is_empty());
    }

    #[test]
    fn register_and_lookup_syntax() {
        let mut reg = DefaultBackendRegistry::new();
        let backend = syntax_platform::RustSyntaxBackend::new();
        let id = syntax_platform::RustSyntaxBackend::backend_id();
        reg.register_syntax(id.clone(), Box::new(backend));

        assert_eq!(reg.syntax_backends("rust"), vec![id.clone()]);
        assert!(reg.syntax_backends("python").is_empty());
        assert_eq!(reg.syntax(&id).language(), "rust");
        assert!(reg.syntax_languages().contains(&"rust"));
    }
}

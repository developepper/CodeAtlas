//! Tree-sitter adapter router for CLI usage.

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};

/// Router backed by real tree-sitter adapters for all supported languages.
pub struct TreeSitterRouter {
    adapters: Vec<TreeSitterAdapter>,
}

impl TreeSitterRouter {
    pub fn new() -> Self {
        let adapters = supported_languages()
            .iter()
            .filter_map(|lang| create_adapter(lang))
            .collect();
        Self { adapters }
    }
}

impl AdapterRouter for TreeSitterRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .filter(|a| a.language() == language)
            .map(|a| a as &dyn LanguageAdapter)
            .collect()
    }
}

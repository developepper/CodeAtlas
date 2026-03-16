//! Dispatch planning: decides which backends to run for each file.

use core_model::{BackendId, FileOnlyReason};
use syntax_platform::PreparedFile;

use crate::registry::BackendRegistry;

/// Runtime context that influences dispatch decisions.
#[derive(Debug, Clone)]
pub struct DispatchContext {
    /// Controls whether syntax backends are invoked.
    pub syntax_policy: SyntaxPolicy,
    /// Controls whether semantic backends are invoked.
    pub semantic_policy: SemanticPolicy,
}

impl Default for DispatchContext {
    fn default() -> Self {
        Self {
            syntax_policy: SyntaxPolicy::EnabledWhenAvailable,
            semantic_policy: SemanticPolicy::EnabledWhenAvailable,
        }
    }
}

/// Policy governing syntax backend participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxPolicy {
    /// Invoke syntax backends when registered and available (normal default).
    EnabledWhenAvailable,
    /// Never invoke syntax backends.
    Disabled,
}

/// Policy governing semantic backend participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticPolicy {
    /// Never invoke semantic backends.
    Disabled,
    /// Invoke semantic backends when registered and available.
    EnabledWhenAvailable,
    /// Require semantic output; treat missing semantic as an error.
    Required,
}

/// Planned execution path for a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPlan {
    /// No extraction will be attempted; file gets file-only indexing.
    FileOnly { reason: FileOnlyReason },
    /// Extraction will be attempted with the listed backends.
    Execute {
        syntax: Vec<BackendId>,
        semantic: Vec<BackendId>,
    },
}

/// Dispatch planner contract.
pub trait DispatchPlanner: Send + Sync {
    fn plan(
        &self,
        file: &PreparedFile,
        registry: &dyn BackendRegistry,
        context: &DispatchContext,
    ) -> ExecutionPlan;
}

/// Default dispatch planner implementing the architecture doc planning rules.
pub struct DefaultDispatchPlanner;

impl DispatchPlanner for DefaultDispatchPlanner {
    fn plan(
        &self,
        file: &PreparedFile,
        registry: &dyn BackendRegistry,
        context: &DispatchContext,
    ) -> ExecutionPlan {
        let syntax_ids = if context.syntax_policy == SyntaxPolicy::Disabled {
            vec![]
        } else {
            registry.syntax_backends(&file.language)
        };

        let semantic_ids = match context.semantic_policy {
            SemanticPolicy::Disabled => vec![],
            SemanticPolicy::EnabledWhenAvailable | SemanticPolicy::Required => {
                registry.semantic_backends(&file.language)
            }
        };

        // If syntax is disabled and no semantic backends available, file-only.
        if syntax_ids.is_empty() && semantic_ids.is_empty() {
            let reason = if context.syntax_policy == SyntaxPolicy::Disabled {
                FileOnlyReason::SyntaxDisabledByPolicy
            } else {
                FileOnlyReason::NoSyntaxBackendRegistered
            };
            return ExecutionPlan::FileOnly { reason };
        }

        ExecutionPlan::Execute {
            syntax: syntax_ids,
            semantic: semantic_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::DefaultBackendRegistry;
    use std::path::PathBuf;

    fn make_file(language: &str) -> PreparedFile {
        PreparedFile {
            relative_path: PathBuf::from("test.rs"),
            absolute_path: PathBuf::from("/tmp/test.rs"),
            language: language.to_string(),
            content: vec![],
        }
    }

    #[test]
    fn no_backends_returns_file_only() {
        let planner = DefaultDispatchPlanner;
        let reg = DefaultBackendRegistry::new();
        let ctx = DispatchContext::default();
        let file = make_file("rust");

        let plan = planner.plan(&file, &reg, &ctx);
        assert!(matches!(
            plan,
            ExecutionPlan::FileOnly {
                reason: FileOnlyReason::NoSyntaxBackendRegistered
            }
        ));
    }

    #[test]
    fn syntax_disabled_returns_file_only() {
        let planner = DefaultDispatchPlanner;
        let reg = DefaultBackendRegistry::new();
        let ctx = DispatchContext {
            syntax_policy: SyntaxPolicy::Disabled,
            semantic_policy: SemanticPolicy::Disabled,
        };
        let file = make_file("rust");

        let plan = planner.plan(&file, &reg, &ctx);
        assert!(matches!(
            plan,
            ExecutionPlan::FileOnly {
                reason: FileOnlyReason::SyntaxDisabledByPolicy
            }
        ));
    }

    #[test]
    fn syntax_backend_registered_returns_execute() {
        let planner = DefaultDispatchPlanner;
        let mut reg = DefaultBackendRegistry::new();
        let id = syntax_platform::RustSyntaxBackend::backend_id();
        reg.register_syntax(
            id.clone(),
            Box::new(syntax_platform::RustSyntaxBackend::new()),
        );
        let ctx = DispatchContext::default();
        let file = make_file("rust");

        let plan = planner.plan(&file, &reg, &ctx);
        match plan {
            ExecutionPlan::Execute { syntax, semantic } => {
                assert_eq!(syntax, vec![id]);
                assert!(semantic.is_empty());
            }
            _ => panic!("expected Execute plan"),
        }
    }
}

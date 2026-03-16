//! Capability classification: determines the tier achieved for a file.

use core_model::CapabilityTier;

use crate::dispatch::ExecutionPlan;
use crate::merge_engine::ExecutionOutcome;

/// Classifies the capability tier achieved for a file based on its
/// execution outcome.
pub trait CapabilityClassifier: Send + Sync {
    fn classify(&self, outcome: &ExecutionOutcome) -> CapabilityTier;
}

/// Default implementation using the architecture doc classification rules.
pub struct DefaultCapabilityClassifier;

impl CapabilityClassifier for DefaultCapabilityClassifier {
    fn classify(&self, outcome: &ExecutionOutcome) -> CapabilityTier {
        // If the plan was FileOnly, no extraction was attempted.
        if matches!(outcome.plan, ExecutionPlan::FileOnly { .. }) {
            return CapabilityTier::FileOnly;
        }

        // Check what actually succeeded.
        let has_syntax = outcome.syntax_attempts.iter().any(|a| a.result.is_ok());
        let has_semantic = outcome.semantic_attempts.iter().any(|a| a.result.is_ok());

        match (has_syntax, has_semantic) {
            (true, true) => CapabilityTier::SyntaxPlusSemantic,
            (true, false) => CapabilityTier::SyntaxOnly,
            (false, true) => CapabilityTier::SemanticOnly,
            (false, false) => CapabilityTier::FileOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_engine::BackendAttempt;
    use core_model::BackendId;
    use semantic_api::SemanticExtraction;
    use syntax_platform::{SyntaxError, SyntaxExtraction};

    fn file_only_outcome() -> ExecutionOutcome {
        ExecutionOutcome {
            plan: ExecutionPlan::FileOnly {
                reason: core_model::FileOnlyReason::NoSyntaxBackendRegistered,
            },
            syntax_attempts: vec![],
            semantic_attempts: vec![],
            merge_result: None,
        }
    }

    fn syntax_only_outcome() -> ExecutionOutcome {
        ExecutionOutcome {
            plan: ExecutionPlan::Execute {
                syntax: vec![BackendId("syntax-rust".into())],
                semantic: vec![],
            },
            syntax_attempts: vec![BackendAttempt {
                backend: BackendId("syntax-rust".into()),
                result: Ok(SyntaxExtraction {
                    language: "rust".into(),
                    symbols: vec![],
                    backend_id: BackendId("syntax-rust".into()),
                }),
            }],
            semantic_attempts: vec![],
            merge_result: None,
        }
    }

    fn semantic_only_outcome() -> ExecutionOutcome {
        ExecutionOutcome {
            plan: ExecutionPlan::Execute {
                syntax: vec![],
                semantic: vec![BackendId("semantic-ts".into())],
            },
            syntax_attempts: vec![],
            semantic_attempts: vec![BackendAttempt {
                backend: BackendId("semantic-ts".into()),
                result: Ok(SemanticExtraction {
                    language: "typescript".into(),
                    symbols: vec![],
                    backend_id: BackendId("semantic-ts".into()),
                    default_confidence: 0.9,
                }),
            }],
            merge_result: None,
        }
    }

    #[test]
    fn file_only_plan_yields_file_only() {
        let c = DefaultCapabilityClassifier;
        assert_eq!(c.classify(&file_only_outcome()), CapabilityTier::FileOnly);
    }

    #[test]
    fn syntax_success_yields_syntax_only() {
        let c = DefaultCapabilityClassifier;
        assert_eq!(
            c.classify(&syntax_only_outcome()),
            CapabilityTier::SyntaxOnly
        );
    }

    #[test]
    fn semantic_success_yields_semantic_only() {
        let c = DefaultCapabilityClassifier;
        assert_eq!(
            c.classify(&semantic_only_outcome()),
            CapabilityTier::SemanticOnly
        );
    }

    #[test]
    fn all_failed_yields_file_only() {
        let c = DefaultCapabilityClassifier;
        let outcome = ExecutionOutcome {
            plan: ExecutionPlan::Execute {
                syntax: vec![BackendId("syntax-rust".into())],
                semantic: vec![],
            },
            syntax_attempts: vec![BackendAttempt {
                backend: BackendId("syntax-rust".into()),
                result: Err(SyntaxError::Parse {
                    path: "test.rs".into(),
                    reason: "failed".into(),
                }),
            }],
            semantic_attempts: vec![],
            merge_result: None,
        };
        assert_eq!(c.classify(&outcome), CapabilityTier::FileOnly);
    }
}

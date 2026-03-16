//! New merge engine for the syntax-platform architecture.
//!
//! Implements the two-phase merge design from the architecture doc:
//! Phase 1 — syntax merge (multiple syntax extractions → SyntaxMergeBaseline)
//! Phase 2 — final merge (baseline + semantic extractions → MergeResult)

use std::collections::HashMap;

use core_model::{BackendId, CapabilityTier, SourceSpan, SymbolKind};
use semantic_api::SemanticExtraction;
use syntax_platform::{SyntaxExtraction, SyntaxMergeBaseline, SyntaxSymbol};
use tracing::debug;

use crate::dispatch::ExecutionPlan;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A canonical merged symbol ready for persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub confidence_score: f32,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
    pub type_refs: Vec<String>,
    pub call_refs: Vec<String>,
}

/// Outcome of a merge decision for a single symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Only one source produced this symbol.
    Unique,
    /// Semantic version won over syntax version.
    SemanticWin,
    /// Syntax version won over semantic version (higher confidence).
    SyntaxWin,
    /// Equal confidence; semantic won by tiebreak rule.
    Tie,
    /// Multiple sources of the same tier; higher confidence won.
    SameTier,
}

/// Provenance metadata for a merged symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedSymbolProvenance {
    pub backend_id: BackendId,
    pub capability_tier: CapabilityTier,
    pub confidence_score: f32,
    pub merge_outcome: MergeOutcome,
}

/// Result of merging syntax and semantic extractions for a single file.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Canonical merged symbols for the file.
    pub symbols: Vec<MergedSymbol>,
    /// Per-symbol provenance, parallel to `symbols`.
    pub provenance: Vec<MergedSymbolProvenance>,
    /// The capability tier achieved for this file.
    pub capability_tier: CapabilityTier,
    /// Number of duplicate symbols resolved during merge.
    pub duplicates_resolved: usize,
}

/// Result of a single backend invocation.
#[derive(Debug)]
pub struct BackendAttempt<T, E> {
    pub backend: BackendId,
    pub result: Result<T, E>,
}

/// Full outcome of executing a plan for a single file.
#[derive(Debug)]
pub struct ExecutionOutcome {
    pub plan: ExecutionPlan,
    pub syntax_attempts: Vec<BackendAttempt<SyntaxExtraction, syntax_platform::SyntaxError>>,
    pub semantic_attempts: Vec<BackendAttempt<SemanticExtraction, semantic_api::SemanticError>>,
    pub merge_result: Option<MergeResult>,
}

// ---------------------------------------------------------------------------
// MergeEngine trait
// ---------------------------------------------------------------------------

/// Merge engine contract.
pub trait MergeEngine: Send + Sync {
    /// Phase 1: Merge multiple syntax extractions into a single baseline.
    /// Returns `None` if `extractions` is empty.
    fn merge_syntax(&self, extractions: &[SyntaxExtraction]) -> Option<SyntaxMergeBaseline>;

    /// Phase 2: Merge the syntax baseline with semantic extractions to
    /// produce the final canonical symbol set.
    fn merge_final(
        &self,
        syntax_baseline: Option<&SyntaxMergeBaseline>,
        semantic: &[SemanticExtraction],
    ) -> MergeResult;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Default merge engine implementing confidence-aware deduplication.
pub struct DefaultMergeEngine;

/// Identity key for deduplication.
type SymbolKey = (String, SymbolKind);

/// A tagged symbol during merge, tracking its origin.
struct TaggedSymbol {
    merged: MergedSymbol,
    backend_id: BackendId,
    tier: CapabilityTier,
    /// Index in the combined input list (lower = higher priority).
    source_index: usize,
}

impl TaggedSymbol {
    fn key(&self) -> SymbolKey {
        (self.merged.qualified_name.clone(), self.merged.kind)
    }
}

impl MergeEngine for DefaultMergeEngine {
    fn merge_syntax(&self, extractions: &[SyntaxExtraction]) -> Option<SyntaxMergeBaseline> {
        if extractions.is_empty() {
            return None;
        }

        // Single syntax backend: trivial pass-through.
        if extractions.len() == 1 {
            let e = &extractions[0];
            return Some(SyntaxMergeBaseline {
                language: e.language.clone(),
                symbols: e.symbols.clone(),
                contributing_backends: vec![e.backend_id.clone()],
            });
        }

        // Multiple syntax backends: deduplicate by (qualified_name, kind).
        let mut all_symbols: Vec<SyntaxSymbol> = Vec::new();
        let mut seen: HashMap<SymbolKey, usize> = HashMap::new();
        let mut backends = Vec::new();

        for extraction in extractions {
            backends.push(extraction.backend_id.clone());
            for sym in &extraction.symbols {
                let key = (sym.qualified_name.clone(), sym.kind);
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(all_symbols.len());
                    all_symbols.push(sym.clone());
                }
            }
        }

        Some(SyntaxMergeBaseline {
            language: extractions[0].language.clone(),
            symbols: all_symbols,
            contributing_backends: backends,
        })
    }

    fn merge_final(
        &self,
        syntax_baseline: Option<&SyntaxMergeBaseline>,
        semantic: &[SemanticExtraction],
    ) -> MergeResult {
        let has_syntax = syntax_baseline.is_some_and(|b| !b.symbols.is_empty());
        let has_semantic = semantic.iter().any(|s| !s.symbols.is_empty());
        let both_tiers = has_syntax && has_semantic;

        // Tag all symbols with their origin.
        let mut tagged: Vec<TaggedSymbol> = Vec::new();
        let mut source_index: usize = 0;

        // Syntax symbols first (lower source_index = higher priority for
        // same-tier ties).
        if let Some(baseline) = syntax_baseline {
            let backend_id = baseline
                .contributing_backends
                .first()
                .cloned()
                .unwrap_or(BackendId("syntax-unknown".to_string()));
            for sym in &baseline.symbols {
                tagged.push(TaggedSymbol {
                    merged: MergedSymbol {
                        name: sym.name.clone(),
                        qualified_name: sym.qualified_name.clone(),
                        kind: sym.kind,
                        span: sym.span,
                        signature: sym.signature.clone(),
                        confidence_score: 0.7, // syntax default
                        docstring: sym.docstring.clone(),
                        parent_qualified_name: sym.parent_qualified_name.clone(),
                        type_refs: vec![],
                        call_refs: vec![],
                    },
                    backend_id: backend_id.clone(),
                    tier: CapabilityTier::SyntaxOnly,
                    source_index,
                });
            }
            source_index += 1;
        }

        // Semantic symbols.
        for extraction in semantic {
            let sem_tier = if both_tiers {
                CapabilityTier::SyntaxPlusSemantic
            } else {
                CapabilityTier::SemanticOnly
            };
            for sym in &extraction.symbols {
                let confidence = sym
                    .confidence_score
                    .unwrap_or(extraction.default_confidence);
                tagged.push(TaggedSymbol {
                    merged: MergedSymbol {
                        name: sym.name.clone(),
                        qualified_name: sym.qualified_name.clone(),
                        kind: sym.kind,
                        span: sym.span,
                        signature: sym.signature.clone(),
                        confidence_score: confidence,
                        docstring: sym.docstring.clone(),
                        parent_qualified_name: sym.parent_qualified_name.clone(),
                        type_refs: sym.type_refs.clone(),
                        call_refs: sym.call_refs.clone(),
                    },
                    backend_id: extraction.backend_id.clone(),
                    tier: sem_tier,
                    source_index,
                });
            }
            source_index += 1;
        }

        // Deduplicate: for each (qualified_name, kind), keep the best.
        let mut best: HashMap<SymbolKey, usize> = HashMap::new();
        let mut outcomes: HashMap<SymbolKey, MergeOutcome> = HashMap::new();
        let mut duplicates_resolved: usize = 0;

        for (i, ts) in tagged.iter().enumerate() {
            let key = ts.key();
            match best.get(&key) {
                None => {
                    best.insert(key.clone(), i);
                    outcomes.insert(key, MergeOutcome::Unique);
                }
                Some(&existing_idx) => {
                    let existing = &tagged[existing_idx];
                    let (winner, loser) = if should_replace(existing, ts) {
                        best.insert(key.clone(), i);
                        (ts, existing)
                    } else {
                        (existing, ts)
                    };
                    duplicates_resolved += 1;

                    let outcome = classify_outcome(winner, loser);
                    outcomes.insert(key, outcome);
                }
            }
        }

        // Collect winners in deterministic order.
        let mut winners: Vec<usize> = best.into_values().collect();
        winners.sort_unstable();

        let symbols: Vec<MergedSymbol> =
            winners.iter().map(|&i| tagged[i].merged.clone()).collect();
        let provenance: Vec<MergedSymbolProvenance> = winners
            .iter()
            .map(|&i| {
                let ts = &tagged[i];
                let key = ts.key();
                let merge_outcome = outcomes.get(&key).copied().unwrap_or(MergeOutcome::Unique);
                MergedSymbolProvenance {
                    backend_id: ts.backend_id.clone(),
                    capability_tier: ts.tier,
                    confidence_score: ts.merged.confidence_score,
                    merge_outcome,
                }
            })
            .collect();

        let capability_tier = if has_syntax && has_semantic {
            CapabilityTier::SyntaxPlusSemantic
        } else if has_semantic {
            CapabilityTier::SemanticOnly
        } else if has_syntax {
            CapabilityTier::SyntaxOnly
        } else {
            CapabilityTier::FileOnly
        };

        debug!(
            symbols = symbols.len(),
            duplicates_resolved,
            tier = %capability_tier,
            "merge complete"
        );

        MergeResult {
            symbols,
            provenance,
            capability_tier,
            duplicates_resolved,
        }
    }
}

/// Returns `true` if `candidate` should replace `existing`.
fn should_replace(existing: &TaggedSymbol, candidate: &TaggedSymbol) -> bool {
    let ec = existing.merged.confidence_score;
    let cc = candidate.merged.confidence_score;

    const EPSILON: f32 = 1e-6;
    let diff = cc - ec;

    if diff > EPSILON {
        return true;
    }
    if diff < -EPSILON {
        return false;
    }

    // Confidence is effectively equal — prefer semantic.
    if candidate.tier.has_semantic() && !existing.tier.has_semantic() {
        return true;
    }
    if !candidate.tier.has_semantic() && existing.tier.has_semantic() {
        return false;
    }

    // Same tier, same confidence — prefer earlier (lower source_index).
    candidate.source_index < existing.source_index
}

fn classify_outcome(winner: &TaggedSymbol, loser: &TaggedSymbol) -> MergeOutcome {
    let same_tier_class = winner.tier.has_semantic() == loser.tier.has_semantic();
    if same_tier_class {
        return MergeOutcome::SameTier;
    }

    const EPSILON: f32 = 1e-6;
    let wc = winner.merged.confidence_score;
    let lc = loser.merged.confidence_score;

    if (wc - lc).abs() < EPSILON {
        MergeOutcome::Tie
    } else if winner.tier.has_semantic() {
        MergeOutcome::SemanticWin
    } else {
        MergeOutcome::SyntaxWin
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::SourceSpan;
    use semantic_api::SemanticSymbol;

    fn make_syntax_sym(name: &str, kind: SymbolKind) -> SyntaxSymbol {
        SyntaxSymbol {
            name: name.to_string(),
            qualified_name: name.to_string(),
            kind,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: format!("fn {name}()"),
            docstring: None,
            parent_qualified_name: None,
        }
    }

    fn make_semantic_sym(name: &str, kind: SymbolKind, confidence: Option<f32>) -> SemanticSymbol {
        SemanticSymbol {
            name: name.to_string(),
            qualified_name: name.to_string(),
            kind,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: format!("fn {name}()"),
            confidence_score: confidence,
            docstring: None,
            parent_qualified_name: None,
            type_refs: vec![],
            call_refs: vec![],
        }
    }

    fn make_syntax_extraction(symbols: Vec<SyntaxSymbol>) -> SyntaxExtraction {
        SyntaxExtraction {
            language: "rust".to_string(),
            symbols,
            backend_id: BackendId("syntax-rust".to_string()),
        }
    }

    fn make_semantic_extraction(symbols: Vec<SemanticSymbol>) -> SemanticExtraction {
        SemanticExtraction {
            language: "rust".to_string(),
            symbols,
            backend_id: BackendId("semantic-rust".to_string()),
            default_confidence: 0.9,
        }
    }

    #[test]
    fn merge_syntax_empty_returns_none() {
        let engine = DefaultMergeEngine;
        assert!(engine.merge_syntax(&[]).is_none());
    }

    #[test]
    fn merge_syntax_single_passthrough() {
        let engine = DefaultMergeEngine;
        let extraction = make_syntax_extraction(vec![make_syntax_sym("foo", SymbolKind::Function)]);
        let baseline = engine.merge_syntax(&[extraction]).unwrap();
        assert_eq!(baseline.symbols.len(), 1);
        assert_eq!(baseline.symbols[0].name, "foo");
        assert_eq!(baseline.contributing_backends.len(), 1);
    }

    #[test]
    fn merge_final_syntax_only() {
        let engine = DefaultMergeEngine;
        let baseline = SyntaxMergeBaseline {
            language: "rust".to_string(),
            symbols: vec![make_syntax_sym("foo", SymbolKind::Function)],
            contributing_backends: vec![BackendId("syntax-rust".into())],
        };
        let result = engine.merge_final(Some(&baseline), &[]);
        assert_eq!(result.capability_tier, CapabilityTier::SyntaxOnly);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "foo");
    }

    #[test]
    fn merge_final_semantic_only() {
        let engine = DefaultMergeEngine;
        let sem =
            make_semantic_extraction(vec![make_semantic_sym("bar", SymbolKind::Function, None)]);
        let result = engine.merge_final(None, &[sem]);
        assert_eq!(result.capability_tier, CapabilityTier::SemanticOnly);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "bar");
    }

    #[test]
    fn merge_final_semantic_wins_over_syntax_with_higher_confidence() {
        let engine = DefaultMergeEngine;
        let baseline = SyntaxMergeBaseline {
            language: "rust".to_string(),
            symbols: vec![make_syntax_sym("foo", SymbolKind::Function)],
            contributing_backends: vec![BackendId("syntax-rust".into())],
        };
        let sem = make_semantic_extraction(vec![make_semantic_sym(
            "foo",
            SymbolKind::Function,
            None, // uses default 0.9 > syntax 0.7
        )]);
        let result = engine.merge_final(Some(&baseline), &[sem]);
        assert_eq!(result.capability_tier, CapabilityTier::SyntaxPlusSemantic);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.duplicates_resolved, 1);
        assert_eq!(
            result.provenance[0].merge_outcome,
            MergeOutcome::SemanticWin
        );
    }

    #[test]
    fn merge_final_non_overlapping_symbols_kept() {
        let engine = DefaultMergeEngine;
        let baseline = SyntaxMergeBaseline {
            language: "rust".to_string(),
            symbols: vec![make_syntax_sym("foo", SymbolKind::Function)],
            contributing_backends: vec![BackendId("syntax-rust".into())],
        };
        let sem = make_semantic_extraction(vec![make_semantic_sym("bar", SymbolKind::Class, None)]);
        let result = engine.merge_final(Some(&baseline), &[sem]);
        assert_eq!(result.symbols.len(), 2);
        assert_eq!(result.duplicates_resolved, 0);
    }

    #[test]
    fn merge_final_no_inputs_yields_file_only() {
        let engine = DefaultMergeEngine;
        let result = engine.merge_final(None, &[]);
        assert_eq!(result.capability_tier, CapabilityTier::FileOnly);
        assert!(result.symbols.is_empty());
    }
}

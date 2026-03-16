//! Confidence-aware merge of adapter outputs.
//!
//! When multiple adapters (e.g. semantic and syntax) both produce results for
//! the same file, this module merges their outputs according to spec §8.2:
//!
//! - Deduplicate symbols by identity (`qualified_name` + `kind`).
//! - Keep the higher-confidence record when the same symbol appears in
//!   multiple outputs.
//! - Preserve provenance (`source_backend`, `capability_tier`) from the
//!   winning output.
//! - Deterministic tie-breaking: prefer semantic over syntax; within the
//!   same quality level, prefer the output that appears earlier in the
//!   input list (i.e. higher-priority adapter).

use std::collections::HashMap;

use adapter_api::{AdapterOutput, ExtractedSymbol};
#[allow(deprecated)]
use core_model::QualityLevel;
use core_model::{CapabilityTier, SymbolKind};
use tracing::debug;

/// Identity key for deduplication: `(qualified_name, kind)`.
type SymbolKey = (String, SymbolKind);

/// A symbol with its provenance metadata, used during merge.
///
/// Internal field `quality_level` is kept as [`QualityLevel`] because
/// [`AdapterOutput`] still uses it. Converted to [`CapabilityTier`] when
/// building the public [`SymbolProvenance`].
struct TaggedSymbol {
    symbol: ExtractedSymbol,
    source_adapter: String,
    #[allow(deprecated)]
    quality_level: QualityLevel,
    /// Default confidence from the producing adapter's capabilities.
    default_confidence: f32,
    /// Index of the adapter output in the input list (lower = higher priority).
    source_index: usize,
}

impl TaggedSymbol {
    /// Effective confidence: per-symbol override if present, else adapter default.
    fn effective_confidence(&self) -> f32 {
        self.symbol
            .confidence_score
            .unwrap_or(self.default_confidence)
    }
}

/// Outcome of a merge decision for a single symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Only one adapter produced this symbol (no overlap).
    Unique,
    /// Multiple adapters produced this symbol; semantic adapter won.
    SemanticWin,
    /// Multiple adapters produced this symbol; syntax adapter won.
    SyntaxWin,
    /// Semantic and syntax adapters both produced this symbol with equal
    /// confidence; tie-broken by adapter priority in favour of semantic.
    Tie,
    /// Multiple adapters of the *same* capability tier both produced this
    /// symbol (e.g. two semantic adapters). Not a semantic-vs-syntax
    /// comparison, so excluded from the win-rate KPI.
    SameTier,
}

/// Per-symbol provenance tracking for merged outputs.
///
/// When symbols from multiple adapters are merged, each symbol retains
/// the `capability_tier` and `source_backend` of the adapter that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolProvenance {
    pub capability_tier: CapabilityTier,
    pub source_backend: String,
    /// The default confidence of the adapter that produced this symbol.
    pub default_confidence: f32,
    /// Whether this symbol was unique to one adapter or won a merge contest.
    pub merge_outcome: MergeOutcome,
}

/// Result of merging multiple adapter outputs for a single file.
///
/// The merged output carries a composite `source_adapter` string when
/// symbols originate from more than one adapter, and the `quality_level`
/// reflects the highest quality among contributing adapters.
///
/// Per-symbol provenance is available in `symbol_provenance`, parallel
/// to `output.symbols`, enabling the persist stage to write accurate
/// per-symbol `capability_tier` and `source_backend` fields.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedOutput {
    /// Merged adapter output with deduplicated symbols.
    pub output: AdapterOutput,
    /// Per-symbol provenance, parallel to `output.symbols`.
    pub symbol_provenance: Vec<SymbolProvenance>,
    /// Number of symbols that were deduplicated (lower-confidence duplicate
    /// was dropped in favour of a higher-confidence version).
    pub duplicates_resolved: usize,
}

/// Merge multiple adapter outputs for the same file.
///
/// `outputs` should be ordered by adapter priority (highest first — semantic
/// before syntax, matching the router's selection order).
///
/// Each tuple is `(adapter_output, default_confidence)` where
/// `default_confidence` is the adapter's `AdapterCapabilities::default_confidence`.
///
/// Returns `None` if `outputs` is empty.
#[allow(deprecated)]
pub fn merge_outputs(outputs: Vec<(AdapterOutput, f32)>) -> Option<MergedOutput> {
    if outputs.is_empty() {
        return None;
    }

    // Fast path: single adapter, no merge needed.
    if outputs.len() == 1 {
        let (output, default_confidence) = outputs.into_iter().next().unwrap();
        let provenance: Vec<SymbolProvenance> = output
            .symbols
            .iter()
            .map(|_| SymbolProvenance {
                capability_tier: CapabilityTier::from(output.quality_level),
                source_backend: output.source_adapter.clone(),
                default_confidence,
                merge_outcome: MergeOutcome::Unique,
            })
            .collect();
        return Some(MergedOutput {
            output,
            symbol_provenance: provenance,
            duplicates_resolved: 0,
        });
    }

    // Tag every symbol with its provenance.
    let mut tagged: Vec<TaggedSymbol> = Vec::new();
    for (idx, (output, default_conf)) in outputs.iter().enumerate() {
        for sym in &output.symbols {
            tagged.push(TaggedSymbol {
                symbol: sym.clone(),
                source_adapter: output.source_adapter.clone(),
                quality_level: output.quality_level,
                default_confidence: *default_conf,
                source_index: idx,
            });
        }
    }

    // Deduplicate: for each (qualified_name, kind), keep the best.
    let mut best: HashMap<SymbolKey, usize> = HashMap::new();
    // Track merge outcomes: which symbols had conflicts and what the result was.
    let mut outcomes: HashMap<SymbolKey, MergeOutcome> = HashMap::new();
    let mut duplicates_resolved: usize = 0;

    for (i, ts) in tagged.iter().enumerate() {
        let key = (ts.symbol.qualified_name.clone(), ts.symbol.kind);
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
                // Classify the merge outcome for KPI tracking.
                // Only cross-quality (semantic vs syntax) contests count
                // toward the win-rate KPI.
                let outcome = if winner.quality_level == loser.quality_level {
                    // Same quality level (e.g. two semantic adapters) —
                    // not a semantic-vs-syntax comparison.
                    MergeOutcome::SameTier
                } else {
                    // Cross-quality contest. Check whether confidence
                    // actually differed or it was a tiebreak.
                    let wc = winner.effective_confidence();
                    let lc = loser.effective_confidence();
                    const EPSILON: f32 = 1e-6;
                    if (wc - lc).abs() < EPSILON {
                        // Equal confidence — semantic won by tiebreak rule.
                        MergeOutcome::Tie
                    } else if winner.quality_level == QualityLevel::Semantic {
                        MergeOutcome::SemanticWin
                    } else {
                        MergeOutcome::SyntaxWin
                    }
                };
                outcomes.insert(key, outcome);
            }
        }
    }

    // Collect winning symbols in a deterministic order: sort by
    // (source_index, original position within that adapter's output).
    // This preserves the adapter priority and intra-adapter symbol order.
    let mut winners: Vec<usize> = best.into_values().collect();
    winners.sort_unstable();

    let merged_symbols: Vec<ExtractedSymbol> =
        winners.iter().map(|&i| tagged[i].symbol.clone()).collect();

    // Determine whether both quality levels contributed to this file's
    // merged output. Used to classify per-symbol provenance and
    // file-level composite provenance.
    let mut has_syntax = false;
    let mut has_semantic = false;
    let mut adapters_used: Vec<&str> = Vec::new();
    let mut highest_quality = QualityLevel::Syntax;
    for &i in &winners {
        let ts = &tagged[i];
        if !adapters_used.contains(&ts.source_adapter.as_str()) {
            adapters_used.push(&ts.source_adapter);
        }
        match ts.quality_level {
            QualityLevel::Semantic => {
                has_semantic = true;
                highest_quality = QualityLevel::Semantic;
            }
            QualityLevel::Syntax => {
                has_syntax = true;
            }
        }
    }

    // Also check all tagged symbols (not just winners) — if the input
    // included both syntax and semantic outputs, even if one tier lost
    // every merge contest, the file still had both tiers contributing.
    for ts in &tagged {
        match ts.quality_level {
            QualityLevel::Semantic => has_semantic = true,
            QualityLevel::Syntax => has_syntax = true,
        }
    }

    let both_tiers = has_syntax && has_semantic;

    // Per-symbol capability_tier reflects how that individual symbol was
    // produced, not the file-level tier:
    //   - Syntax-sourced symbol → SyntaxOnly (always)
    //   - Semantic-sourced symbol in a file with both tiers →
    //     SyntaxPlusSemantic (syntax context was available)
    //   - Semantic-sourced symbol in a semantic-only file →
    //     SemanticOnly (no syntax baseline existed)
    let symbol_provenance: Vec<SymbolProvenance> = winners
        .iter()
        .map(|&i| {
            let ts = &tagged[i];
            let key = (ts.symbol.qualified_name.clone(), ts.symbol.kind);
            let merge_outcome = outcomes.get(&key).copied().unwrap_or(MergeOutcome::Unique);
            let tier = match ts.quality_level {
                QualityLevel::Syntax => CapabilityTier::SyntaxOnly,
                QualityLevel::Semantic => {
                    if both_tiers {
                        CapabilityTier::SyntaxPlusSemantic
                    } else {
                        CapabilityTier::SemanticOnly
                    }
                }
            };
            SymbolProvenance {
                capability_tier: tier,
                source_backend: ts.source_adapter.clone(),
                default_confidence: ts.default_confidence,
                merge_outcome,
            }
        })
        .collect();

    let source_adapter = if adapters_used.len() == 1 {
        adapters_used[0].to_string()
    } else {
        adapters_used.join("+")
    };

    debug!(
        adapters = %source_adapter,
        symbols = merged_symbols.len(),
        duplicates_resolved,
        "merge complete"
    );

    Some(MergedOutput {
        output: AdapterOutput {
            symbols: merged_symbols,
            source_adapter,
            quality_level: highest_quality,
        },
        symbol_provenance,
        duplicates_resolved,
    })
}

/// Returns `true` if `candidate` should replace `existing`.
///
/// Tie-breaking rules (spec §8.2):
/// 1. Higher effective confidence wins.
/// 2. If confidence ties, semantic quality wins over syntax.
/// 3. If still tied, the adapter with lower source_index wins (higher
///    priority in the router's selection order).
#[allow(deprecated)]
fn should_replace(existing: &TaggedSymbol, candidate: &TaggedSymbol) -> bool {
    let ec = existing.effective_confidence();
    let cc = candidate.effective_confidence();

    // Use an epsilon for float comparison to avoid instability.
    const EPSILON: f32 = 1e-6;
    let diff = cc - ec;

    if diff > EPSILON {
        return true;
    }
    if diff < -EPSILON {
        return false;
    }

    // Confidence is effectively equal — prefer semantic.
    if candidate.quality_level == QualityLevel::Semantic
        && existing.quality_level == QualityLevel::Syntax
    {
        return true;
    }
    if candidate.quality_level == QualityLevel::Syntax
        && existing.quality_level == QualityLevel::Semantic
    {
        return false;
    }

    // Same quality level, same confidence — prefer earlier adapter (lower index).
    candidate.source_index < existing.source_index
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use adapter_api::SourceSpan;
    use core_model::SymbolKind;

    fn make_symbol(name: &str, kind: SymbolKind, confidence: Option<f32>) -> ExtractedSymbol {
        ExtractedSymbol {
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
        }
    }

    #[allow(deprecated)]
    fn make_output(
        adapter: &str,
        quality: QualityLevel,
        symbols: Vec<ExtractedSymbol>,
    ) -> AdapterOutput {
        AdapterOutput {
            symbols,
            source_adapter: adapter.to_string(),
            quality_level: quality,
        }
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(merge_outputs(vec![]).is_none());
    }

    #[test]
    #[allow(deprecated)]
    fn single_output_passes_through() {
        let sym = make_symbol("foo", SymbolKind::Function, None);
        let output = make_output("syntax-v1", QualityLevel::Syntax, vec![sym.clone()]);
        let merged = merge_outputs(vec![(output, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        assert_eq!(merged.output.symbols[0].name, "foo");
        assert_eq!(merged.output.source_adapter, "syntax-v1");
        assert_eq!(merged.output.quality_level, QualityLevel::Syntax);
        assert_eq!(merged.duplicates_resolved, 0);
    }

    #[test]
    #[allow(deprecated)]
    fn semantic_wins_over_syntax_for_same_symbol() {
        let syntax_sym = make_symbol("foo", SymbolKind::Function, None);
        let semantic_sym = make_symbol("foo", SymbolKind::Function, None);

        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syntax_sym]);
        let semantic_out = make_output("semantic-v1", QualityLevel::Semantic, vec![semantic_sym]);

        // Semantic first (higher priority).
        let merged = merge_outputs(vec![(semantic_out, 0.9), (syntax_out, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        assert_eq!(merged.output.source_adapter, "semantic-v1");
        assert_eq!(merged.output.quality_level, QualityLevel::Semantic);
        assert_eq!(merged.duplicates_resolved, 1);
    }

    #[test]
    #[allow(deprecated)]
    fn higher_confidence_wins_regardless_of_quality_level() {
        // Syntax adapter with very high per-symbol confidence.
        let syntax_sym = make_symbol("foo", SymbolKind::Function, Some(0.99));
        let semantic_sym = make_symbol("foo", SymbolKind::Function, Some(0.5));

        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syntax_sym]);
        let semantic_out = make_output("semantic-v1", QualityLevel::Semantic, vec![semantic_sym]);

        let merged = merge_outputs(vec![(semantic_out, 0.9), (syntax_out, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        // Syntax wins because its per-symbol confidence (0.99) > semantic's (0.5).
        assert_eq!(merged.output.symbols[0].confidence_score, Some(0.99));
        assert_eq!(merged.duplicates_resolved, 1);
    }

    #[test]
    #[allow(deprecated)]
    fn non_overlapping_symbols_are_all_kept() {
        let syn_sym = make_symbol("foo", SymbolKind::Function, None);
        let sem_sym = make_symbol("bar", SymbolKind::Class, None);

        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syn_sym]);
        let semantic_out = make_output("semantic-v1", QualityLevel::Semantic, vec![sem_sym]);

        let merged = merge_outputs(vec![(semantic_out, 0.9), (syntax_out, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 2);
        assert_eq!(merged.duplicates_resolved, 0);
        // Composite source adapter.
        assert!(merged.output.source_adapter.contains('+'));
        assert_eq!(merged.output.quality_level, QualityLevel::Semantic);
    }

    #[test]
    #[allow(deprecated)]
    fn same_name_different_kind_are_distinct() {
        let func = make_symbol("foo", SymbolKind::Function, None);
        let constant = make_symbol("foo", SymbolKind::Constant, None);

        let out1 = make_output("syntax-v1", QualityLevel::Syntax, vec![func]);
        let out2 = make_output("semantic-v1", QualityLevel::Semantic, vec![constant]);

        let merged = merge_outputs(vec![(out2, 0.9), (out1, 0.7)]).unwrap();
        assert_eq!(merged.output.symbols.len(), 2);
        assert_eq!(merged.duplicates_resolved, 0);
    }

    #[test]
    #[allow(deprecated)]
    fn tie_breaking_prefers_semantic_quality() {
        // Both adapters with same confidence, different quality levels.
        let syntax_sym = make_symbol("foo", SymbolKind::Function, Some(0.8));
        let semantic_sym = make_symbol("foo", SymbolKind::Function, Some(0.8));

        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syntax_sym]);
        let semantic_out = make_output("semantic-v1", QualityLevel::Semantic, vec![semantic_sym]);

        // Syntax registered first (lower index), but semantic should still win.
        let merged = merge_outputs(vec![(syntax_out, 0.7), (semantic_out, 0.9)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        assert_eq!(merged.output.quality_level, QualityLevel::Semantic);
    }

    #[test]
    #[allow(deprecated)]
    fn tie_breaking_prefers_earlier_adapter_same_quality() {
        let sym_a = make_symbol("foo", SymbolKind::Function, Some(0.85));
        let sym_b = make_symbol("foo", SymbolKind::Function, Some(0.85));

        let out_a = make_output("semantic-a", QualityLevel::Semantic, vec![sym_a]);
        let out_b = make_output("semantic-b", QualityLevel::Semantic, vec![sym_b]);

        let merged = merge_outputs(vec![(out_a, 0.9), (out_b, 0.9)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        // Earlier adapter (index 0) wins the tie.
        assert_eq!(merged.output.source_adapter, "semantic-a");
    }

    #[test]
    #[allow(deprecated)]
    fn default_confidence_used_when_no_override() {
        // Semantic adapter default 0.9 > syntax adapter default 0.7.
        let syn_sym = make_symbol("foo", SymbolKind::Function, None);
        let sem_sym = make_symbol("foo", SymbolKind::Function, None);

        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syn_sym]);
        let semantic_out = make_output("semantic-v1", QualityLevel::Semantic, vec![sem_sym]);

        let merged = merge_outputs(vec![(semantic_out, 0.9), (syntax_out, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 1);
        // Semantic wins with default confidence 0.9 > 0.7.
        assert_eq!(merged.output.source_adapter, "semantic-v1");
    }

    #[test]
    #[allow(deprecated)]
    fn multiple_symbols_mixed_overlap() {
        // Semantic finds: foo (function), bar (class)
        // Syntax finds:  foo (function), baz (method)
        // Expected: foo from semantic (higher confidence), bar from semantic, baz from syntax.
        let sem_foo = make_symbol("foo", SymbolKind::Function, None);
        let sem_bar = make_symbol("bar", SymbolKind::Class, None);
        let syn_foo = make_symbol("foo", SymbolKind::Function, None);
        let syn_baz = make_symbol("baz", SymbolKind::Method, None);

        let semantic_out = make_output(
            "semantic-v1",
            QualityLevel::Semantic,
            vec![sem_foo, sem_bar],
        );
        let syntax_out = make_output("syntax-v1", QualityLevel::Syntax, vec![syn_foo, syn_baz]);

        let merged = merge_outputs(vec![(semantic_out, 0.9), (syntax_out, 0.7)]).unwrap();

        assert_eq!(merged.output.symbols.len(), 3);
        assert_eq!(merged.duplicates_resolved, 1);

        let names: Vec<&str> = merged
            .output
            .symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"baz"));
    }

    #[test]
    #[allow(deprecated)]
    fn deterministic_symbol_ordering() {
        // Run merge twice, verify same output order.
        let build = || {
            let sem = make_output(
                "semantic-v1",
                QualityLevel::Semantic,
                vec![
                    make_symbol("alpha", SymbolKind::Function, None),
                    make_symbol("gamma", SymbolKind::Class, None),
                ],
            );
            let syn = make_output(
                "syntax-v1",
                QualityLevel::Syntax,
                vec![
                    make_symbol("beta", SymbolKind::Method, None),
                    make_symbol("gamma", SymbolKind::Class, None),
                ],
            );
            merge_outputs(vec![(sem, 0.9), (syn, 0.7)]).unwrap()
        };

        let run1 = build();
        let run2 = build();

        let names1: Vec<&str> = run1
            .output
            .symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let names2: Vec<&str> = run2
            .output
            .symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names1, names2);
    }

    #[test]
    #[allow(deprecated)]
    fn quality_level_reflects_highest_contributor() {
        // All symbols from syntax only → quality stays Syntax.
        let syn_a = make_symbol("foo", SymbolKind::Function, None);
        let syn_b = make_symbol("bar", SymbolKind::Function, None);

        let out_a = make_output("syntax-a", QualityLevel::Syntax, vec![syn_a]);
        let out_b = make_output("syntax-b", QualityLevel::Syntax, vec![syn_b]);

        let merged = merge_outputs(vec![(out_a, 0.7), (out_b, 0.75)]).unwrap();
        assert_eq!(merged.output.quality_level, QualityLevel::Syntax);
    }
}

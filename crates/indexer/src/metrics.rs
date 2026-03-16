//! Capability tier metrics computation.
//!
//! Computes quality KPIs from parse output to quantify the value of
//! semantic adapters over syntax baselines.

use std::collections::BTreeMap;

use crate::merge_engine::MergeOutcome;
use crate::stage::ParseOutput;

/// Capability tier metrics for an indexing run.
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityTierMetrics {
    /// Total symbols extracted across all files.
    pub total_symbols: usize,
    /// Symbols produced by semantic backends.
    pub semantic_symbols: usize,
    /// Symbols produced by syntax backends.
    pub syntax_symbols: usize,
    /// Percentage of symbols from semantic backends (0.0..=100.0).
    pub semantic_coverage_percent: f32,
    /// Sum of confidence scores across all symbols.
    pub total_confidence: f32,
    /// Average confidence score across all symbols.
    pub avg_confidence: f32,
    /// Number of duplicate symbols resolved during merge.
    pub duplicates_resolved: usize,
    /// Symbol count per source backend (e.g. "syntax-rust" → 42).
    pub backend_symbol_counts: BTreeMap<String, usize>,
    /// Number of files that have at least one semantic symbol.
    pub files_with_semantic: usize,
    /// Total number of parsed files.
    pub total_files: usize,
    /// Number of files indexed at file level only (no symbols extracted).
    pub files_file_only: usize,
    /// Number of overlapping symbols where semantic adapter won the merge.
    pub wins: usize,
    /// Number of overlapping symbols where syntax adapter won the merge.
    pub losses: usize,
    /// Number of overlapping symbols resolved by tie-breaking.
    pub ties: usize,
    /// Semantic-vs-syntax win rate for overlapping symbols (0.0..=1.0).
    pub win_rate: f32,
}

/// Computes capability tier metrics from parse output.
pub fn compute_tier_metrics(parse_output: &ParseOutput) -> CapabilityTierMetrics {
    let mut total_symbols: usize = 0;
    let mut semantic_symbols: usize = 0;
    let mut syntax_symbols: usize = 0;
    let mut total_confidence: f32 = 0.0;
    let mut backend_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut files_with_semantic: usize = 0;
    let mut files_file_only: usize = 0;
    let mut duplicates_resolved: usize = 0;

    let mut wins: usize = 0;
    let mut losses: usize = 0;
    let mut ties: usize = 0;

    for parsed in &parse_output.parsed_files {
        let sym_count = parsed.merge_result.symbols.len();
        total_symbols += sym_count;
        duplicates_resolved += parsed.merge_result.duplicates_resolved;

        if sym_count == 0 {
            files_file_only += 1;
        }

        let mut file_has_semantic = false;

        for provenance in &parsed.symbol_provenance {
            if provenance.capability_tier.has_semantic() {
                semantic_symbols += 1;
                file_has_semantic = true;
            } else {
                syntax_symbols += 1;
            }

            total_confidence += provenance.confidence_score;

            *backend_counts
                .entry(provenance.backend_id.0.clone())
                .or_insert(0) += 1;

            match provenance.merge_outcome {
                MergeOutcome::SemanticWin => wins += 1,
                MergeOutcome::SyntaxWin => losses += 1,
                MergeOutcome::Tie => ties += 1,
                MergeOutcome::Unique | MergeOutcome::SameTier => {}
            }
        }

        if file_has_semantic {
            files_with_semantic += 1;
        }
    }

    let overlap_total = wins + losses + ties;
    let win_rate = if overlap_total > 0 {
        wins as f32 / overlap_total as f32
    } else {
        0.0
    };

    let semantic_coverage_percent = if total_symbols > 0 {
        (semantic_symbols as f32 / total_symbols as f32) * 100.0
    } else {
        0.0
    };

    let avg_confidence = if total_symbols > 0 {
        total_confidence / total_symbols as f32
    } else {
        0.0
    };

    CapabilityTierMetrics {
        total_symbols,
        semantic_symbols,
        syntax_symbols,
        semantic_coverage_percent,
        total_confidence,
        avg_confidence,
        duplicates_resolved,
        backend_symbol_counts: backend_counts,
        files_with_semantic,
        total_files: parse_output.parsed_files.len(),
        files_file_only,
        wins,
        losses,
        ties,
        win_rate,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use core_model::{BackendId, CapabilityTier, SourceSpan, SymbolKind};

    use crate::merge_engine::{MergeOutcome, MergeResult, MergedSymbol, MergedSymbolProvenance};
    use crate::stage::{ParseOutput, ParsedFile};

    use super::*;

    fn make_merged_symbol(name: &str) -> MergedSymbol {
        MergedSymbol {
            name: name.to_string(),
            qualified_name: name.to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: format!("fn {name}()"),
            confidence_score: 0.7,
            docstring: None,
            parent_qualified_name: None,
            type_refs: vec![],
            call_refs: vec![],
        }
    }

    fn make_parsed_file(
        path: &str,
        symbols: Vec<MergedSymbol>,
        provenances: Vec<MergedSymbolProvenance>,
        tier: CapabilityTier,
        duplicates: usize,
    ) -> ParsedFile {
        ParsedFile {
            relative_path: PathBuf::from(path),
            language: "rust".to_string(),
            symbol_provenance: provenances,
            merge_result: MergeResult {
                symbols,
                provenance: vec![], // not used by metrics
                capability_tier: tier,
                duplicates_resolved: duplicates,
            },
            content_hash: "sha256:test".to_string(),
            content: Vec::new(),
        }
    }

    #[test]
    fn empty_parse_output_yields_zero_metrics() {
        let output = ParseOutput {
            parsed_files: vec![],
            file_errors: vec![],
        };
        let metrics = compute_tier_metrics(&output);
        assert_eq!(metrics.total_symbols, 0);
        assert_eq!(metrics.semantic_symbols, 0);
        assert!((metrics.win_rate - 0.0).abs() < 1e-6);
    }

    #[test]
    fn all_syntax_yields_zero_semantic_coverage() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/main.rs",
                vec![make_merged_symbol("foo"), make_merged_symbol("bar")],
                vec![
                    MergedSymbolProvenance {
                        backend_id: BackendId("syntax-rust".into()),
                        capability_tier: CapabilityTier::SyntaxOnly,
                        confidence_score: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    MergedSymbolProvenance {
                        backend_id: BackendId("syntax-rust".into()),
                        capability_tier: CapabilityTier::SyntaxOnly,
                        confidence_score: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
                CapabilityTier::SyntaxOnly,
                0,
            )],
            file_errors: vec![],
        };

        let metrics = compute_tier_metrics(&output);
        assert_eq!(metrics.total_symbols, 2);
        assert_eq!(metrics.semantic_symbols, 0);
        assert_eq!(metrics.syntax_symbols, 2);
        assert!((metrics.semantic_coverage_percent - 0.0).abs() < 1e-6);
        assert!((metrics.avg_confidence - 0.7).abs() < 1e-6);
    }

    #[test]
    fn win_rate_semantic_wins() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![make_merged_symbol("Config")],
                vec![MergedSymbolProvenance {
                    backend_id: BackendId("semantic-ts".into()),
                    capability_tier: CapabilityTier::SyntaxPlusSemantic,
                    confidence_score: 0.9,
                    merge_outcome: MergeOutcome::SemanticWin,
                }],
                CapabilityTier::SyntaxPlusSemantic,
                1,
            )],
            file_errors: vec![],
        };

        let metrics = compute_tier_metrics(&output);
        assert_eq!(metrics.wins, 1);
        assert!((metrics.win_rate - 1.0).abs() < 1e-6);
        assert_eq!(metrics.duplicates_resolved, 1);
    }
}

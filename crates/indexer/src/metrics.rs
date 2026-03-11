//! Semantic coverage metrics computation.
//!
//! Computes quality KPIs from parse output to quantify the value of
//! semantic adapters over syntax baselines (spec §13.1, §15.3).

use std::collections::BTreeMap;

use core_model::QualityLevel;

use crate::merge::MergeOutcome;
use crate::stage::ParseOutput;

/// Semantic coverage metrics for an indexing run.
///
/// Quantifies how many symbols were produced by semantic adapters versus
/// syntax-only adapters, and breaks down contributions per adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticCoverageMetrics {
    /// Total symbols extracted across all files.
    pub total_symbols: usize,
    /// Symbols produced by semantic adapters.
    pub semantic_symbols: usize,
    /// Symbols produced by syntax adapters.
    pub syntax_symbols: usize,
    /// Percentage of symbols from semantic adapters (0.0..=100.0).
    pub semantic_coverage_percent: f32,
    /// Sum of confidence scores across all symbols.
    pub total_confidence: f32,
    /// Average confidence score across all symbols.
    pub avg_confidence: f32,
    /// Number of duplicate symbols resolved during merge.
    pub duplicates_resolved: usize,
    /// Symbol count per source adapter (e.g. "semantic-typescript-v1" → 42).
    pub adapter_symbol_counts: BTreeMap<String, usize>,
    /// Number of files that have at least one semantic symbol.
    pub files_with_semantic: usize,
    /// Total number of parsed files.
    pub total_files: usize,
    /// Number of overlapping symbols where semantic adapter won the merge.
    pub wins: usize,
    /// Number of overlapping symbols where syntax adapter won the merge.
    pub losses: usize,
    /// Number of overlapping symbols resolved by tie-breaking (same quality level).
    pub ties: usize,
    /// Semantic-vs-syntax win rate for overlapping symbols (0.0..=1.0).
    /// Computed as wins / (wins + losses + ties). NaN-safe: 0.0 when no overlaps.
    pub win_rate: f32,
}

/// Computes semantic coverage metrics from parse output.
pub fn compute_coverage(parse_output: &ParseOutput) -> SemanticCoverageMetrics {
    let mut total_symbols: usize = 0;
    let mut semantic_symbols: usize = 0;
    let mut syntax_symbols: usize = 0;
    let mut total_confidence: f32 = 0.0;
    let mut adapter_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut files_with_semantic: usize = 0;

    for parsed in &parse_output.parsed_files {
        let sym_count = parsed.symbol_provenance.len();
        total_symbols += sym_count;

        let mut file_has_semantic = false;

        for (i, provenance) in parsed.symbol_provenance.iter().enumerate() {
            match provenance.quality_level {
                QualityLevel::Semantic => {
                    semantic_symbols += 1;
                    file_has_semantic = true;
                }
                QualityLevel::Syntax => {
                    syntax_symbols += 1;
                }
            }

            // Accumulate confidence from the actual symbol output.
            if let Some(sym) = parsed.output.symbols.get(i) {
                total_confidence += sym
                    .confidence_score
                    .unwrap_or(provenance.default_confidence);
            }

            *adapter_counts
                .entry(provenance.source_adapter.clone())
                .or_insert(0) += 1;
        }

        if file_has_semantic {
            files_with_semantic += 1;
        }
    }

    let duplicates_resolved = count_duplicates_resolved(parse_output);

    // Compute win-rate from merge outcomes across all files.
    let mut wins: usize = 0;
    let mut losses: usize = 0;
    let mut ties: usize = 0;
    for parsed in &parse_output.parsed_files {
        for provenance in &parsed.symbol_provenance {
            match provenance.merge_outcome {
                MergeOutcome::SemanticWin => wins += 1,
                MergeOutcome::SyntaxWin => losses += 1,
                MergeOutcome::Tie => ties += 1,
                MergeOutcome::Unique | MergeOutcome::SameQuality => {}
            }
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

    SemanticCoverageMetrics {
        total_symbols,
        semantic_symbols,
        syntax_symbols,
        semantic_coverage_percent,
        total_confidence,
        avg_confidence,
        duplicates_resolved,
        adapter_symbol_counts: adapter_counts,
        files_with_semantic,
        total_files: parse_output.parsed_files.len(),
        wins,
        losses,
        ties,
        win_rate,
    }
}

/// Counts duplicates resolved by checking files where multiple adapters
/// contributed symbols (indicated by composite source_adapter strings).
fn count_duplicates_resolved(parse_output: &ParseOutput) -> usize {
    let mut count = 0;
    for parsed in &parse_output.parsed_files {
        // A composite source_adapter like "semantic-v1+syntax-v1" means
        // merge happened. Count unique adapters per file to estimate.
        let mut adapters_in_file: Vec<&str> = Vec::new();
        for p in &parsed.symbol_provenance {
            if !adapters_in_file.contains(&p.source_adapter.as_str()) {
                adapters_in_file.push(&p.source_adapter);
            }
        }
        // If multiple adapters contributed, at least some merging occurred.
        if adapters_in_file.len() > 1 {
            // Conservative: count the number of symbols from the non-primary
            // adapter as a lower bound on duplicates that were resolved.
            // The actual count would require the MergedOutput.duplicates_resolved
            // field which is not preserved in ParsedFile.
            count += adapters_in_file.len() - 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use adapter_api::{AdapterOutput, ExtractedSymbol, SourceSpan};
    use core_model::{QualityLevel, SymbolKind};

    use crate::merge::{MergeOutcome, SymbolProvenance};
    use crate::stage::{ParseOutput, ParsedFile};

    use super::*;

    fn make_symbol(name: &str, confidence: f32) -> ExtractedSymbol {
        ExtractedSymbol {
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
            confidence_score: Some(confidence),
            docstring: None,
            parent_qualified_name: None,
        }
    }

    fn make_parsed_file(
        path: &str,
        symbols: Vec<ExtractedSymbol>,
        provenances: Vec<SymbolProvenance>,
    ) -> ParsedFile {
        ParsedFile {
            relative_path: PathBuf::from(path),
            language: "rust".to_string(),
            output: AdapterOutput {
                symbols,
                source_adapter: "test-adapter".to_string(),
                quality_level: QualityLevel::Syntax,
            },
            symbol_provenance: provenances,
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

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.total_symbols, 0);
        assert_eq!(metrics.semantic_symbols, 0);
        assert_eq!(metrics.syntax_symbols, 0);
        assert!((metrics.semantic_coverage_percent - 0.0).abs() < 1e-6);
        assert!((metrics.avg_confidence - 0.0).abs() < 1e-6);
        assert_eq!(metrics.files_with_semantic, 0);
        assert_eq!(metrics.total_files, 0);
        assert!(metrics.adapter_symbol_counts.is_empty());
        assert_eq!(metrics.wins, 0);
        assert_eq!(metrics.losses, 0);
        assert_eq!(metrics.ties, 0);
        assert!((metrics.win_rate - 0.0).abs() < 1e-6);
    }

    #[test]
    fn all_syntax_yields_zero_semantic_coverage() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/main.rs",
                vec![make_symbol("foo", 0.7), make_symbol("bar", 0.7)],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-treesitter-rust".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-treesitter-rust".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.total_symbols, 2);
        assert_eq!(metrics.semantic_symbols, 0);
        assert_eq!(metrics.syntax_symbols, 2);
        assert!((metrics.semantic_coverage_percent - 0.0).abs() < 1e-6);
        assert!((metrics.avg_confidence - 0.7).abs() < 1e-6);
        assert_eq!(metrics.files_with_semantic, 0);
        assert_eq!(metrics.total_files, 1);
        assert_eq!(
            metrics.adapter_symbol_counts.get("syntax-treesitter-rust"),
            Some(&2)
        );
    }

    #[test]
    fn mixed_semantic_and_syntax() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![
                    make_symbol("Config", 0.9),
                    make_symbol("create", 0.9),
                    make_symbol("MAX_SIZE", 0.7),
                ],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-typescript-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-typescript-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-treesitter-typescript".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.total_symbols, 3);
        assert_eq!(metrics.semantic_symbols, 2);
        assert_eq!(metrics.syntax_symbols, 1);

        let expected_pct = (2.0 / 3.0) * 100.0;
        assert!((metrics.semantic_coverage_percent - expected_pct).abs() < 0.1);

        let expected_avg = (0.9 + 0.9 + 0.7) / 3.0;
        assert!((metrics.avg_confidence - expected_avg).abs() < 1e-6);

        assert_eq!(metrics.files_with_semantic, 1);
        assert_eq!(
            metrics.adapter_symbol_counts.get("semantic-typescript-v1"),
            Some(&2)
        );
        assert_eq!(
            metrics
                .adapter_symbol_counts
                .get("syntax-treesitter-typescript"),
            Some(&1)
        );
    }

    #[test]
    fn multiple_files_aggregated() {
        let output = ParseOutput {
            parsed_files: vec![
                make_parsed_file(
                    "src/a.ts",
                    vec![make_symbol("a", 0.9)],
                    vec![SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-typescript-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    }],
                ),
                make_parsed_file(
                    "src/b.rs",
                    vec![make_symbol("b", 0.7)],
                    vec![SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-treesitter-rust".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    }],
                ),
            ],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.total_symbols, 2);
        assert_eq!(metrics.semantic_symbols, 1);
        assert_eq!(metrics.syntax_symbols, 1);
        assert!((metrics.semantic_coverage_percent - 50.0).abs() < 0.1);
        assert_eq!(metrics.files_with_semantic, 1);
        assert_eq!(metrics.total_files, 2);
    }

    #[test]
    fn all_semantic_yields_full_coverage() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.kt",
                vec![make_symbol("Config", 0.9), make_symbol("create", 0.9)],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-kotlin-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-kotlin-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert!((metrics.semantic_coverage_percent - 100.0).abs() < 1e-6);
        assert!((metrics.avg_confidence - 0.9).abs() < 1e-6);
        assert_eq!(metrics.files_with_semantic, 1);
    }

    #[test]
    fn default_confidence_used_when_symbol_has_none() {
        let sym = ExtractedSymbol {
            name: "foo".to_string(),
            qualified_name: "foo".to_string(),
            kind: SymbolKind::Function,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: "fn foo()".to_string(),
            confidence_score: None, // no override
            docstring: None,
            parent_qualified_name: None,
        };

        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/main.rs",
                vec![sym],
                vec![SymbolProvenance {
                    quality_level: QualityLevel::Syntax,
                    source_adapter: "syntax-treesitter-rust".to_string(),
                    default_confidence: 0.7,
                    merge_outcome: MergeOutcome::Unique,
                }],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        // Should fall back to default_confidence of 0.7.
        assert!((metrics.avg_confidence - 0.7).abs() < 1e-6);
    }

    #[test]
    fn adapter_counts_are_sorted() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.rs",
                vec![make_symbol("a", 0.9), make_symbol("b", 0.7)],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-v1".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        let keys: Vec<&String> = metrics.adapter_symbol_counts.keys().collect();
        // BTreeMap is sorted.
        assert_eq!(keys, vec!["semantic-v1", "syntax-v1"]);
    }

    #[test]
    fn metrics_computation_is_deterministic() {
        let build = || ParseOutput {
            parsed_files: vec![
                make_parsed_file(
                    "src/a.ts",
                    vec![make_symbol("x", 0.9), make_symbol("y", 0.85)],
                    vec![
                        SymbolProvenance {
                            quality_level: QualityLevel::Semantic,
                            source_adapter: "semantic-v1".to_string(),
                            default_confidence: 0.9,
                            merge_outcome: MergeOutcome::Unique,
                        },
                        SymbolProvenance {
                            quality_level: QualityLevel::Semantic,
                            source_adapter: "semantic-v1".to_string(),
                            default_confidence: 0.9,
                            merge_outcome: MergeOutcome::Unique,
                        },
                    ],
                ),
                make_parsed_file(
                    "src/b.rs",
                    vec![make_symbol("z", 0.7)],
                    vec![SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-v1".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    }],
                ),
            ],
            file_errors: vec![],
        };

        let m1 = compute_coverage(&build());
        let m2 = compute_coverage(&build());

        assert_eq!(m1, m2);
    }

    #[test]
    fn win_rate_all_semantic_wins() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![make_symbol("Config", 0.9), make_symbol("create", 0.9)],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SemanticWin,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SemanticWin,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.wins, 2);
        assert_eq!(metrics.losses, 0);
        assert_eq!(metrics.ties, 0);
        assert!((metrics.win_rate - 1.0).abs() < 1e-6);
    }

    #[test]
    fn win_rate_mixed_outcomes() {
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![
                    make_symbol("Config", 0.9),
                    make_symbol("create", 0.7),
                    make_symbol("MAX", 0.8),
                ],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SemanticWin,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-v1".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::SyntaxWin,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Tie,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.wins, 1);
        assert_eq!(metrics.losses, 1);
        assert_eq!(metrics.ties, 1);
        // win_rate = 1 / 3
        assert!((metrics.win_rate - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn win_rate_unique_symbols_excluded() {
        // Unique symbols (no merge contest) should not affect win rate.
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![
                    make_symbol("Config", 0.9),
                    make_symbol("create", 0.9),
                    make_symbol("helper", 0.7),
                ],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SemanticWin,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-v1".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::Unique,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Syntax,
                        source_adapter: "syntax-v1".to_string(),
                        default_confidence: 0.7,
                        merge_outcome: MergeOutcome::Unique,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        assert_eq!(metrics.wins, 1);
        assert_eq!(metrics.losses, 0);
        assert_eq!(metrics.ties, 0);
        // Only 1 overlap, semantic won → 100% win rate.
        assert!((metrics.win_rate - 1.0).abs() < 1e-6);
    }

    #[test]
    fn win_rate_excludes_same_quality_overlaps() {
        // Two semantic adapters conflict on "Config", and one semantic
        // wins over syntax on "create". The same-quality overlap must
        // not appear in win/loss/tie or affect win_rate.
        let output = ParseOutput {
            parsed_files: vec![make_parsed_file(
                "src/lib.ts",
                vec![make_symbol("Config", 0.9), make_symbol("create", 0.9)],
                vec![
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-a".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SameQuality,
                    },
                    SymbolProvenance {
                        quality_level: QualityLevel::Semantic,
                        source_adapter: "semantic-a".to_string(),
                        default_confidence: 0.9,
                        merge_outcome: MergeOutcome::SemanticWin,
                    },
                ],
            )],
            file_errors: vec![],
        };

        let metrics = compute_coverage(&output);
        // SameQuality is excluded from the KPI denominator.
        assert_eq!(metrics.wins, 1);
        assert_eq!(metrics.losses, 0);
        assert_eq!(metrics.ties, 0);
        assert!((metrics.win_rate - 1.0).abs() < 1e-6);
    }
}

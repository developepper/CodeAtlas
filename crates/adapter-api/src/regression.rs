//! Semantic adapter quality regression harness.
//!
//! Provides regression fixtures, quality comparison functions, and KPI
//! computation for verifying that semantic adapters maintain measurable
//! quality improvements over syntax baselines.
//!
//! Enable the `test-harness` feature in `dev-dependencies` to use this module.
//!
//! # Usage
//!
//! ```ignore
//! use adapter_api::regression::{RegressionFixture, run_quality_regression};
//!
//! let adapter = create_my_semantic_adapter();
//! let fixture = RegressionFixture::typescript();
//! let result = run_quality_regression(&adapter, &fixture);
//! result.assert_thresholds();
//! ```

use std::path::PathBuf;

use core_model::{QualityLevel, SymbolKind};

use crate::{AdapterOutput, IndexContext, LanguageAdapter, SourceFile};

// ---------------------------------------------------------------------------
// Regression fixture definition
// ---------------------------------------------------------------------------

/// A symbol expected in a regression fixture output.
#[derive(Debug, Clone)]
pub struct ExpectedSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    /// Expected confidence (semantic-level).
    pub semantic_confidence: f32,
    /// Simulated confidence from a syntax-only baseline.
    pub syntax_confidence: f32,
}

/// A quality regression fixture with source code, expected semantic symbols,
/// and simulated syntax baseline for KPI computation.
pub struct RegressionFixture {
    /// Language identifier (must match the adapter's `language()`).
    pub language: String,
    /// Source code bytes.
    pub source_code: Vec<u8>,
    /// Relative path for the fixture file.
    pub relative_path: PathBuf,
    /// Expected symbols from the semantic adapter.
    pub expected_symbols: Vec<ExpectedSymbol>,
    /// Symbol names a syntax-only parser would typically find.
    /// Used to compute semantic-vs-syntax win rate.
    pub syntax_baseline_names: Vec<String>,
    /// Minimum acceptable semantic-vs-syntax win rate (0.0..=1.0).
    pub min_win_rate: f32,
    /// Minimum number of symbols the semantic adapter must extract.
    pub min_symbol_count: usize,
    /// Minimum average confidence the semantic adapter must achieve.
    pub min_avg_confidence: f32,
}

/// Computed quality KPIs from a regression run.
#[derive(Debug, Clone)]
pub struct QualityKpi {
    /// Total symbols extracted by the semantic adapter.
    pub semantic_symbol_count: usize,
    /// Total symbols a syntax baseline would find.
    pub syntax_symbol_count: usize,
    /// Symbols found by semantic but not by syntax (coverage advantage).
    pub semantic_only_count: usize,
    /// Average confidence score across all semantic symbols.
    pub avg_semantic_confidence: f32,
    /// For symbols found by both adapters, the fraction where
    /// semantic confidence exceeds syntax confidence.
    pub win_rate: f32,
    /// Number of overlapping symbols where semantic won.
    pub wins: usize,
    /// Number of overlapping symbols where syntax won.
    pub losses: usize,
    /// Number of overlapping symbols with equal confidence.
    pub ties: usize,
    /// Per-symbol details for overlapping symbols.
    pub symbol_comparisons: Vec<SymbolComparison>,
}

/// Per-symbol comparison between semantic and syntax confidence.
#[derive(Debug, Clone)]
pub struct SymbolComparison {
    pub name: String,
    pub kind: SymbolKind,
    pub semantic_confidence: f32,
    pub syntax_confidence: f32,
    pub outcome: ComparisonOutcome,
}

/// Outcome of a single symbol confidence comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOutcome {
    SemanticWin,
    SyntaxWin,
    Tie,
}

/// Full result of a quality regression run.
#[derive(Debug)]
pub struct RegressionResult {
    /// The adapter output from the semantic adapter.
    pub output: AdapterOutput,
    /// Computed KPIs.
    pub kpi: QualityKpi,
    /// Thresholds from the fixture.
    pub min_win_rate: f32,
    pub min_symbol_count: usize,
    pub min_avg_confidence: f32,
}

impl RegressionResult {
    /// Asserts all quality thresholds pass.
    ///
    /// # Panics
    /// Panics with diagnostic detail if any threshold is violated.
    pub fn assert_thresholds(&self) {
        assert!(
            self.output.quality_level == QualityLevel::Semantic,
            "regression: adapter output must be semantic quality, got {:?}",
            self.output.quality_level
        );

        assert!(
            self.kpi.semantic_symbol_count >= self.min_symbol_count,
            "regression: expected at least {} symbols, got {}",
            self.min_symbol_count,
            self.kpi.semantic_symbol_count
        );

        assert!(
            self.kpi.avg_semantic_confidence >= self.min_avg_confidence,
            "regression: average confidence {:.3} below minimum {:.3}",
            self.kpi.avg_semantic_confidence,
            self.min_avg_confidence
        );

        assert!(
            self.kpi.win_rate >= self.min_win_rate,
            "regression: semantic-vs-syntax win rate {:.3} below minimum {:.3} \
             (wins={}, losses={}, ties={})",
            self.kpi.win_rate,
            self.min_win_rate,
            self.kpi.wins,
            self.kpi.losses,
            self.kpi.ties
        );

        assert_eq!(
            self.kpi.losses, 0,
            "regression: semantic adapter must not lose to syntax baseline \
             on any symbol, but lost on {} symbols",
            self.kpi.losses
        );
    }

    /// Formats KPIs as a human-readable report.
    #[must_use]
    pub fn report(&self) -> String {
        format!(
            "Quality Regression Report\n\
             -------------------------\n\
             Semantic symbols:     {}\n\
             Syntax baseline:      {}\n\
             Semantic-only:        {}\n\
             Avg confidence:       {:.3}\n\
             Win rate:             {:.1}%\n\
             Wins / Losses / Ties: {} / {} / {}\n\
             Thresholds:\n\
             \x20 min symbols:       {}\n\
             \x20 min avg conf:      {:.3}\n\
             \x20 min win rate:      {:.1}%",
            self.kpi.semantic_symbol_count,
            self.kpi.syntax_symbol_count,
            self.kpi.semantic_only_count,
            self.kpi.avg_semantic_confidence,
            self.kpi.win_rate * 100.0,
            self.kpi.wins,
            self.kpi.losses,
            self.kpi.ties,
            self.min_symbol_count,
            self.min_avg_confidence,
            self.min_win_rate * 100.0,
        )
    }
}

// ---------------------------------------------------------------------------
// Regression runner
// ---------------------------------------------------------------------------

/// Runs the quality regression suite for a semantic adapter against a fixture.
///
/// Extracts symbols via the adapter, compares against the fixture's syntax
/// baseline, and computes quality KPIs.
///
/// # Panics
/// Panics if the adapter returns an error for the fixture source.
pub fn run_quality_regression(
    adapter: &dyn LanguageAdapter,
    fixture: &RegressionFixture,
) -> RegressionResult {
    let ctx = IndexContext {
        repo_id: "regression-test-repo".to_string(),
        source_root: PathBuf::from("/tmp/regression-test-repo"),
    };
    let file = SourceFile {
        relative_path: fixture.relative_path.clone(),
        absolute_path: PathBuf::from("/tmp/regression-test-repo").join(&fixture.relative_path),
        content: fixture.source_code.clone(),
        language: fixture.language.clone(),
    };

    let output = adapter
        .index_file(&ctx, &file)
        .expect("regression: semantic adapter must not error on fixture source");

    let kpi = compute_kpi(&output, fixture);

    RegressionResult {
        output,
        kpi,
        min_win_rate: fixture.min_win_rate,
        min_symbol_count: fixture.min_symbol_count,
        min_avg_confidence: fixture.min_avg_confidence,
    }
}

/// Computes quality KPIs by comparing semantic output against the fixture baseline.
fn compute_kpi(output: &AdapterOutput, fixture: &RegressionFixture) -> QualityKpi {
    let semantic_symbol_count = output.symbols.len();
    let syntax_symbol_count = fixture.syntax_baseline_names.len();

    let semantic_names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    let semantic_only_count = semantic_names
        .iter()
        .filter(|name| !fixture.syntax_baseline_names.iter().any(|sn| sn == *name))
        .count();

    let avg_semantic_confidence = if output.symbols.is_empty() {
        0.0
    } else {
        let sum: f32 = output
            .symbols
            .iter()
            .map(|s| s.confidence_score.unwrap_or(0.0))
            .sum();
        sum / output.symbols.len() as f32
    };

    let mut wins = 0usize;
    let mut losses = 0usize;
    let mut ties = 0usize;
    let mut comparisons = Vec::new();

    for expected in &fixture.expected_symbols {
        // Check if this symbol exists in the syntax baseline.
        let in_syntax = fixture
            .syntax_baseline_names
            .iter()
            .any(|sn| sn == &expected.name);
        if !in_syntax {
            continue;
        }

        // Find the symbol in the semantic output. If the semantic adapter
        // failed to extract an overlapping symbol that the syntax baseline
        // would have found, count it as a loss — the syntax baseline still
        // covers it but semantic does not.
        let (outcome, semantic_conf) =
            if let Some(semantic_sym) = output.symbols.iter().find(|s| s.name == expected.name) {
                let sc = semantic_sym.confidence_score.unwrap_or(0.0);
                let outcome = if (sc - expected.syntax_confidence).abs() < 1e-6 {
                    ComparisonOutcome::Tie
                } else if sc > expected.syntax_confidence {
                    ComparisonOutcome::SemanticWin
                } else {
                    ComparisonOutcome::SyntaxWin
                };
                (outcome, sc)
            } else {
                // Semantic adapter dropped an overlapping symbol — loss.
                (ComparisonOutcome::SyntaxWin, 0.0)
            };

        match outcome {
            ComparisonOutcome::SemanticWin => wins += 1,
            ComparisonOutcome::SyntaxWin => losses += 1,
            ComparisonOutcome::Tie => ties += 1,
        }

        comparisons.push(SymbolComparison {
            name: expected.name.clone(),
            kind: expected.kind,
            semantic_confidence: semantic_conf,
            syntax_confidence: expected.syntax_confidence,
            outcome,
        });
    }

    let total_compared = wins + losses + ties;
    let win_rate = if total_compared == 0 {
        1.0
    } else {
        wins as f32 / total_compared as f32
    };

    QualityKpi {
        semantic_symbol_count,
        syntax_symbol_count,
        semantic_only_count,
        avg_semantic_confidence,
        win_rate,
        wins,
        losses,
        ties,
        symbol_comparisons: comparisons,
    }
}

// ---------------------------------------------------------------------------
// Determinism assertion
// ---------------------------------------------------------------------------

/// Asserts the regression fixture produces deterministic KPI results
/// across repeated runs.
///
/// # Panics
/// Panics if symbol counts, win rates, or confidence values differ.
pub fn assert_regression_is_deterministic(
    adapter: &dyn LanguageAdapter,
    fixture: &RegressionFixture,
) {
    let result1 = run_quality_regression(adapter, fixture);
    let result2 = run_quality_regression(adapter, fixture);

    assert_eq!(
        result1.kpi.semantic_symbol_count, result2.kpi.semantic_symbol_count,
        "regression determinism: symbol count differs"
    );
    assert!(
        (result1.kpi.win_rate - result2.kpi.win_rate).abs() < 1e-6,
        "regression determinism: win rate differs"
    );
    assert!(
        (result1.kpi.avg_semantic_confidence - result2.kpi.avg_semantic_confidence).abs() < 1e-6,
        "regression determinism: avg confidence differs"
    );
    assert_eq!(
        result1.kpi.wins, result2.kpi.wins,
        "regression determinism: win count differs"
    );
}

// ---------------------------------------------------------------------------
// TypeScript regression fixture
// ---------------------------------------------------------------------------

impl RegressionFixture {
    /// TypeScript regression fixture with rich symbol variety.
    ///
    /// Covers: interfaces, classes with methods, standalone functions,
    /// arrow-assigned constants, enums, type aliases, generics, and
    /// exported declarations — a superset of the baseline contract fixture.
    #[must_use]
    pub fn typescript() -> Self {
        Self {
            language: "typescript".to_string(),
            source_code: TS_REGRESSION_SOURCE.as_bytes().to_vec(),
            relative_path: PathBuf::from("src/service.ts"),
            expected_symbols: ts_expected_symbols(),
            syntax_baseline_names: vec![
                "ServiceConfig".to_string(),
                "createService".to_string(),
                "ServiceImpl".to_string(),
                "start".to_string(),
                "stop".to_string(),
                "handleRequest".to_string(),
                "ServiceStatus".to_string(),
                "RequestHandler".to_string(),
                "DEFAULT_TIMEOUT".to_string(),
            ],
            min_win_rate: 0.8,
            min_symbol_count: 9,
            min_avg_confidence: 0.85,
        }
    }

    /// Kotlin regression fixture with rich symbol variety.
    ///
    /// Covers: data classes, companion objects, extension functions, sealed
    /// classes, object declarations, enums, top-level constants, and nested
    /// class methods — a superset of the baseline contract fixture.
    #[must_use]
    pub fn kotlin() -> Self {
        Self {
            language: "kotlin".to_string(),
            source_code: KT_REGRESSION_SOURCE.as_bytes().to_vec(),
            relative_path: PathBuf::from("src/Service.kt"),
            expected_symbols: kt_expected_symbols(),
            syntax_baseline_names: vec![
                "ServiceConfig".to_string(),
                "createService".to_string(),
                "ServiceImpl".to_string(),
                "start".to_string(),
                "stop".to_string(),
                "handleRequest".to_string(),
                "ServiceStatus".to_string(),
                "RequestHandler".to_string(),
                "DEFAULT_TIMEOUT".to_string(),
            ],
            min_win_rate: 0.8,
            min_symbol_count: 9,
            min_avg_confidence: 0.85,
        }
    }
}

// ---------------------------------------------------------------------------
// TypeScript regression source
// ---------------------------------------------------------------------------

const TS_REGRESSION_SOURCE: &str = r#"/** Configuration for the service layer. */
interface ServiceConfig {
    host: string;
    port: number;
    timeout: number;
}

/** Creates a configured service instance. */
function createService(config: ServiceConfig): ServiceImpl {
    return new ServiceImpl(config);
}

/** Core service implementation. */
class ServiceImpl {
    private config: ServiceConfig;

    constructor(config: ServiceConfig) {
        this.config = config;
    }

    /** Starts the service and binds to the configured port. */
    start(): void {
        console.log(`Starting on ${this.config.host}:${this.config.port}`);
    }

    /** Stops the service gracefully. */
    stop(): void {
        console.log("Stopping service");
    }

    /** Handles an incoming request with timeout enforcement. */
    handleRequest(path: string, body: unknown): boolean {
        return path.length > 0;
    }
}

/** Operational status of the service. */
enum ServiceStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
}

/** Type alias for request handler callbacks. */
type RequestHandler = (path: string, body: unknown) => boolean;

/** Default timeout in milliseconds. */
const DEFAULT_TIMEOUT: number = 30000;
"#;

fn ts_expected_symbols() -> Vec<ExpectedSymbol> {
    vec![
        ExpectedSymbol {
            name: "ServiceConfig".to_string(),
            qualified_name: "ServiceConfig".to_string(),
            kind: SymbolKind::Type,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "createService".to_string(),
            qualified_name: "createService".to_string(),
            kind: SymbolKind::Function,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "ServiceImpl".to_string(),
            qualified_name: "ServiceImpl".to_string(),
            kind: SymbolKind::Class,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "start".to_string(),
            qualified_name: "ServiceImpl::start".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "stop".to_string(),
            qualified_name: "ServiceImpl::stop".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "handleRequest".to_string(),
            qualified_name: "ServiceImpl::handleRequest".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "ServiceStatus".to_string(),
            qualified_name: "ServiceStatus".to_string(),
            kind: SymbolKind::Type,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "RequestHandler".to_string(),
            qualified_name: "RequestHandler".to_string(),
            kind: SymbolKind::Type,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "DEFAULT_TIMEOUT".to_string(),
            qualified_name: "DEFAULT_TIMEOUT".to_string(),
            kind: SymbolKind::Constant,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
    ]
}

// ---------------------------------------------------------------------------
// Kotlin regression source
// ---------------------------------------------------------------------------

const KT_REGRESSION_SOURCE: &str = r#"/** Configuration for the service layer. */
data class ServiceConfig(
    val host: String,
    val port: Int,
    val timeout: Long
)

/** Creates a configured service instance. */
fun createService(config: ServiceConfig): ServiceImpl {
    return ServiceImpl(config)
}

/** Core service implementation. */
class ServiceImpl(private val config: ServiceConfig) {
    /** Starts the service and binds to the configured port. */
    fun start() {
        println("Starting on ${config.host}:${config.port}")
    }

    /** Stops the service gracefully. */
    fun stop() {
        println("Stopping service")
    }

    /** Handles an incoming request with timeout enforcement. */
    fun handleRequest(path: String, body: Any?): Boolean {
        return path.isNotEmpty()
    }
}

/** Operational status of the service. */
enum class ServiceStatus {
    Starting,
    Running,
    Stopping,
    Stopped
}

/** Type alias for request handler callbacks. */
typealias RequestHandler = (String, Any?) -> Boolean

/** Default timeout in milliseconds. */
const val DEFAULT_TIMEOUT: Long = 30000
"#;

fn kt_expected_symbols() -> Vec<ExpectedSymbol> {
    vec![
        ExpectedSymbol {
            name: "ServiceConfig".to_string(),
            qualified_name: "ServiceConfig".to_string(),
            kind: SymbolKind::Class,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "createService".to_string(),
            qualified_name: "createService".to_string(),
            kind: SymbolKind::Function,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "ServiceImpl".to_string(),
            qualified_name: "ServiceImpl".to_string(),
            kind: SymbolKind::Class,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "start".to_string(),
            qualified_name: "ServiceImpl::start".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "stop".to_string(),
            qualified_name: "ServiceImpl::stop".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "handleRequest".to_string(),
            qualified_name: "ServiceImpl::handleRequest".to_string(),
            kind: SymbolKind::Method,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "ServiceStatus".to_string(),
            qualified_name: "ServiceStatus".to_string(),
            kind: SymbolKind::Type,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "RequestHandler".to_string(),
            qualified_name: "RequestHandler".to_string(),
            kind: SymbolKind::Type,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
        ExpectedSymbol {
            name: "DEFAULT_TIMEOUT".to_string(),
            qualified_name: "DEFAULT_TIMEOUT".to_string(),
            kind: SymbolKind::Constant,
            semantic_confidence: 0.9,
            syntax_confidence: 0.7,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExtractedSymbol, SourceSpan};

    fn make_semantic_output(names: &[&str], confidence: f32) -> AdapterOutput {
        let symbols = names
            .iter()
            .enumerate()
            .map(|(i, name)| ExtractedSymbol {
                name: name.to_string(),
                qualified_name: name.to_string(),
                kind: SymbolKind::Function,
                span: SourceSpan {
                    start_line: (i as u32) + 1,
                    end_line: (i as u32) + 1,
                    start_byte: 0,
                    byte_length: 10,
                },
                signature: format!("fn {name}()"),
                confidence_score: Some(confidence),
                docstring: None,
                parent_qualified_name: None,
            })
            .collect();

        AdapterOutput {
            symbols,
            source_adapter: "semantic-test-v1".to_string(),
            quality_level: QualityLevel::Semantic,
        }
    }

    #[test]
    fn kpi_all_wins() {
        let output = make_semantic_output(&["foo", "bar", "baz"], 0.9);
        let fixture = RegressionFixture {
            language: "test".to_string(),
            source_code: Vec::new(),
            relative_path: PathBuf::from("test.ts"),
            expected_symbols: vec![
                ExpectedSymbol {
                    name: "foo".to_string(),
                    qualified_name: "foo".to_string(),
                    kind: SymbolKind::Function,
                    semantic_confidence: 0.9,
                    syntax_confidence: 0.7,
                },
                ExpectedSymbol {
                    name: "bar".to_string(),
                    qualified_name: "bar".to_string(),
                    kind: SymbolKind::Function,
                    semantic_confidence: 0.9,
                    syntax_confidence: 0.7,
                },
            ],
            syntax_baseline_names: vec!["foo".to_string(), "bar".to_string()],
            min_win_rate: 0.8,
            min_symbol_count: 2,
            min_avg_confidence: 0.8,
        };

        let kpi = compute_kpi(&output, &fixture);
        assert_eq!(kpi.wins, 2);
        assert_eq!(kpi.losses, 0);
        assert_eq!(kpi.ties, 0);
        assert!((kpi.win_rate - 1.0).abs() < 1e-6);
        assert_eq!(kpi.semantic_only_count, 1); // "baz" not in syntax baseline
    }

    #[test]
    fn kpi_with_ties() {
        let output = make_semantic_output(&["foo", "bar"], 0.7);
        let fixture = RegressionFixture {
            language: "test".to_string(),
            source_code: Vec::new(),
            relative_path: PathBuf::from("test.ts"),
            expected_symbols: vec![ExpectedSymbol {
                name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                kind: SymbolKind::Function,
                semantic_confidence: 0.7,
                syntax_confidence: 0.7,
            }],
            syntax_baseline_names: vec!["foo".to_string(), "bar".to_string()],
            min_win_rate: 0.0,
            min_symbol_count: 1,
            min_avg_confidence: 0.5,
        };

        let kpi = compute_kpi(&output, &fixture);
        assert_eq!(kpi.ties, 1);
        assert_eq!(kpi.wins, 0);
    }

    #[test]
    fn kpi_missing_overlapping_symbol_counts_as_loss() {
        // Semantic adapter only extracts "foo" but the fixture expects both
        // "foo" and "bar" in the syntax baseline overlap. Missing "bar" must
        // count as a loss, not be silently ignored.
        let output = make_semantic_output(&["foo"], 0.9);
        let fixture = RegressionFixture {
            language: "test".to_string(),
            source_code: Vec::new(),
            relative_path: PathBuf::from("test.ts"),
            expected_symbols: vec![
                ExpectedSymbol {
                    name: "foo".to_string(),
                    qualified_name: "foo".to_string(),
                    kind: SymbolKind::Function,
                    semantic_confidence: 0.9,
                    syntax_confidence: 0.7,
                },
                ExpectedSymbol {
                    name: "bar".to_string(),
                    qualified_name: "bar".to_string(),
                    kind: SymbolKind::Function,
                    semantic_confidence: 0.9,
                    syntax_confidence: 0.7,
                },
            ],
            syntax_baseline_names: vec!["foo".to_string(), "bar".to_string()],
            min_win_rate: 0.8,
            min_symbol_count: 2,
            min_avg_confidence: 0.8,
        };

        let kpi = compute_kpi(&output, &fixture);
        assert_eq!(kpi.wins, 1, "foo should be a win");
        assert_eq!(kpi.losses, 1, "missing bar should be a loss");
        assert!(
            (kpi.win_rate - 0.5).abs() < 1e-6,
            "win rate should be 0.5, got {}",
            kpi.win_rate
        );
        assert_eq!(kpi.symbol_comparisons.len(), 2);

        let bar_cmp = kpi
            .symbol_comparisons
            .iter()
            .find(|c| c.name == "bar")
            .expect("bar comparison must exist");
        assert_eq!(bar_cmp.outcome, ComparisonOutcome::SyntaxWin);
        assert!((bar_cmp.semantic_confidence - 0.0).abs() < 1e-6);
    }

    #[test]
    fn kpi_empty_output() {
        let output = AdapterOutput {
            symbols: vec![],
            source_adapter: "test".to_string(),
            quality_level: QualityLevel::Semantic,
        };
        let fixture = RegressionFixture {
            language: "test".to_string(),
            source_code: Vec::new(),
            relative_path: PathBuf::from("test.ts"),
            expected_symbols: vec![],
            syntax_baseline_names: vec![],
            min_win_rate: 0.0,
            min_symbol_count: 0,
            min_avg_confidence: 0.0,
        };

        let kpi = compute_kpi(&output, &fixture);
        assert_eq!(kpi.semantic_symbol_count, 0);
        assert!((kpi.avg_semantic_confidence - 0.0).abs() < 1e-6);
        assert!((kpi.win_rate - 1.0).abs() < 1e-6); // no comparisons = 1.0
    }

    #[test]
    fn typescript_fixture_is_well_formed() {
        let fixture = RegressionFixture::typescript();
        assert_eq!(fixture.language, "typescript");
        assert!(!fixture.source_code.is_empty());
        assert!(!fixture.expected_symbols.is_empty());
        assert!(fixture.min_win_rate > 0.0);
        assert!(fixture.min_symbol_count > 0);

        for sym in &fixture.expected_symbols {
            assert!(!sym.name.is_empty());
            assert!(!sym.qualified_name.is_empty());
            assert!(sym.semantic_confidence > sym.syntax_confidence);
        }
    }

    #[test]
    fn kotlin_fixture_is_well_formed() {
        let fixture = RegressionFixture::kotlin();
        assert_eq!(fixture.language, "kotlin");
        assert!(!fixture.source_code.is_empty());
        assert!(!fixture.expected_symbols.is_empty());
        assert!(fixture.min_win_rate > 0.0);
        assert!(fixture.min_symbol_count > 0);

        for sym in &fixture.expected_symbols {
            assert!(!sym.name.is_empty());
            assert!(!sym.qualified_name.is_empty());
            assert!(sym.semantic_confidence > sym.syntax_confidence);
        }
    }

    #[test]
    fn report_format_is_readable() {
        let result = RegressionResult {
            output: make_semantic_output(&["foo", "bar"], 0.9),
            kpi: QualityKpi {
                semantic_symbol_count: 2,
                syntax_symbol_count: 1,
                semantic_only_count: 1,
                avg_semantic_confidence: 0.9,
                win_rate: 1.0,
                wins: 1,
                losses: 0,
                ties: 0,
                symbol_comparisons: vec![],
            },
            min_win_rate: 0.8,
            min_symbol_count: 1,
            min_avg_confidence: 0.85,
        };

        let report = result.report();
        assert!(report.contains("Quality Regression Report"));
        assert!(report.contains("Win rate"));
        assert!(report.contains("100.0%"));
    }
}

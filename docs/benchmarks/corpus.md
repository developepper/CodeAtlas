# Benchmark Corpus

Curated corpus and quality KPI baseline for CodeAtlas (spec sections 15.2, 15.3).

## Corpus Selection Criteria

The benchmark corpus is designed to exercise semantic adapter quality across
representative code patterns. Each fixture covers the same set of semantic
constructs to enable cross-language quality comparison.

### Required constructs per language fixture

| Construct          | TypeScript equivalent | Kotlin equivalent   |
| ------------------ | --------------------- | ------------------- |
| Data type          | `interface`           | `data class`        |
| Top-level function | `function`            | `fun`               |
| Class              | `class`               | `class`             |
| Methods (3)        | `method`              | `fun` (member)      |
| Enum               | `enum`                | `enum class`        |
| Type alias         | `type`                | `typealias`         |
| Constant           | `const`               | `const val`         |

Each fixture produces **9 expected symbols** covering all constructs.

## Corpus Fixtures

### TypeScript (`RegressionFixture::typescript()`)

Source: inline in `crates/adapter-api/src/regression.rs`

Defines a service abstraction with:
- `ServiceConfig` interface (3 properties)
- `createService()` factory function
- `ServiceImpl` class with `start()`, `stop()`, `handleRequest()` methods
- `ServiceStatus` enum (4 variants)
- `RequestHandler` type alias
- `DEFAULT_TIMEOUT` constant

### Kotlin (`RegressionFixture::kotlin()`)

Source: inline in `crates/adapter-api/src/regression.rs`

Mirrors the TypeScript fixture with idiomatic Kotlin equivalents:
- `ServiceConfig` data class
- `createService()` top-level function
- `ServiceImpl` class with same methods
- `ServiceStatus` enum class
- `RequestHandler` typealias
- `DEFAULT_TIMEOUT` const val

## Quality KPI Thresholds

These thresholds are enforced by `RegressionResult::assert_thresholds()` and
gated in CI via the quality regression tests.

| KPI                       | Threshold | Rationale                                |
| ------------------------- | --------- | ---------------------------------------- |
| Min symbol count          | 9         | All expected constructs must be extracted |
| Min avg confidence        | 0.85      | Semantic adapters must exceed syntax      |
| Min win rate              | 80%       | Semantic must win most overlap contests   |
| Max losses                | 0         | No regressions vs syntax baseline         |

## Performance SLO Targets (spec section 13.4)

Enforced by `perf_thresholds` tests in CI:

| Metric                       | Target        | Test location                          |
| ---------------------------- | ------------- | -------------------------------------- |
| `search_symbols` p95         | < 300 ms      | `query-engine/tests/perf_thresholds`   |
| `get_symbol` p95             | < 120 ms      | `query-engine/tests/perf_thresholds`   |
| `file_outline` p95           | < 200 ms      | `query-engine/tests/perf_thresholds`   |
| Full pipeline (20 files)     | < 10 s        | `indexer/tests/perf_thresholds`        |
| Incremental reindex (no-op)  | < 5 s         | `indexer/tests/perf_thresholds`        |
| Discovery (100 files)        | < 500 ms p95  | `repo-walker/tests/perf_thresholds`    |

## KPI Reporting

### CI automated report (fixture-based regression)

The `rust-quality-kpi` CI job generates a quality KPI report on every push
and PR. It runs the TypeScript and Kotlin regression suites against their
curated fixtures, captures the per-language regression report output, and
uploads the result as a `quality-kpi-report` build artifact.

The report includes per-adapter win rate, confidence scores, symbol counts,
and pass/fail status against the thresholds above.

### CLI quality report (repository coverage)

For live repositories, generate a coverage report with:

```bash
codeatlas quality-report <path> [--db <path>] [--git-diff]
```

This indexes the repository and reports semantic coverage metrics:
- Semantic vs syntax symbol counts and coverage percentage
- Average confidence score
- Semantic-vs-syntax win rate (from merge outcomes)
- Per-adapter symbol breakdown
- Pass/fail evaluation against KPI thresholds

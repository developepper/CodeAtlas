# Operations Runbook

This runbook documents the key operational workflows for CodeAtlas in its
current local-first deployment mode.

It is intended to be actionable for maintainers and local operators.

## Scope

This runbook covers:

- local environment preparation
- indexing and query workflows
- MCP server setup and troubleshooting
- semantic adapter setup and diagnosis
- quality KPI reporting
- schema/index maintenance workflows
- CI and milestone-closeout checks

It does not cover future hosted service operations such as tenancy, managed
databases, or production alerting.

## 1. Local Environment Setup

### Install CodeAtlas

See the [README Installation section](../../README.md#installation) for
supported install paths (`cargo install`, GitHub Release binaries, or building
from source).

### Required baseline tools (for building from source)

- Rust toolchain compatible with workspace `rust-version` (1.81+)
- `cargo`
- Git

### Optional tools for semantic coverage

- TypeScript semantic adapter:
  - `tsserver`
  - or project-local `node_modules/.bin/tsserver`
- Kotlin semantic adapter:
  - `java`
  - Kotlin bridge JAR

### Useful environment variables

- `TSSERVER_PATH`
- `JAVA_HOME`
- `KOTLIN_BRIDGE_JAR`
- `CODEATLAS_LOG`
- `CODEATLAS_LOG_FORMAT`
- `CODEATLAS_OTEL`

## 2. Index a Repository

Full index:

```bash
cargo run -p cli -- index <repo-path>
```

Optional custom DB path:

```bash
cargo run -p cli -- index <repo-path> --db <db-path>
```

Optional git-diff acceleration:

```bash
cargo run -p cli -- index <repo-path> --git-diff
```

Expected output includes:

- files discovered / parsed / errored
- symbols extracted
- semantic coverage summary
- confidence summary
- per-adapter breakdown when available

## 3. Query the Local Index

Query commands default to the shared store at `~/.codeatlas/metadata.db`.
Use `--db <path>` to override. Commands that scope results to a repository
require `--repo <repo-id>`.

Examples:

```bash
cargo run -p cli -- search-symbols <query> --repo <repo-id>
cargo run -p cli -- get-symbol <symbol-id>
cargo run -p cli -- file-outline <path> --repo <repo-id>
cargo run -p cli -- file-tree --repo <repo-id>
cargo run -p cli -- repo-outline --repo <repo-id>
```

Note: `get-symbol` does not require `--repo` — symbol IDs include the repo
prefix (e.g. `my-app//src/lib.rs::Config#class`) so they are globally unique.

Use the MCP server path when integrating with agent clients instead of direct
CLI query commands.

## 4. Run the MCP Server

### Start the server

```bash
codeatlas mcp serve --db ~/.codeatlas/metadata.db
```

The server reads newline-delimited JSON-RPC from stdin and writes responses to
stdout. All diagnostics go to stderr.

### Verify it works

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | codeatlas mcp serve --db ~/.codeatlas/metadata.db
```

Expected: a JSON response with `"protocolVersion":"2025-11-25"`.

### Configure an AI client

See the [README MCP Server Setup](../../README.md#mcp-server-setup) for
copy-paste configuration examples for Claude Desktop, Cursor, and OpenAI Codex CLI.
See [MCP Client Compatibility Notes](../architecture/mcp-client-compatibility.md)
for the currently documented interoperability shims and rationale.

### Troubleshoot startup failures

| Symptom | Cause | Fix |
|---------|-------|-----|
| `database not found` | DB path does not exist | Run `codeatlas index <repo>` first (default: `~/.codeatlas/metadata.db`) |
| `database is not readable` | Permission denied | Check file permissions |
| `failed to open database` | Corrupt or non-SQLite file | Re-run `codeatlas index <repo>` |
| `database path is a directory` | Path points to directory | Use the `.db` file path |
| `--db <path> is required` | Missing `--db` flag | Add `--db /path/to/index.db` |
| No stdout response | Client uses Content-Length framing | Use newline-delimited JSON-RPC |
| Empty tool results | Wrong `repo_id` | `repo_id` is derived from the directory name used during indexing |

### What the server does not support

- HTTP, gRPC, or WebSocket transports
- Content-Length framed MCP (2024-11-05 transport)
- Authentication or multi-user access
- Remote/hosted serving

## 5. Generate a Repository Quality Report

For live repository coverage metrics:

```bash
cargo run -p cli -- quality-report <repo-path>
```

Optional flags:

```bash
cargo run -p cli -- quality-report <repo-path> --db <db-path> --git-diff
```

The report includes:

- semantic vs syntax symbol counts
- semantic coverage percentage
- average confidence
- semantic-vs-syntax win rate from merge outcomes
- per-adapter contribution breakdown
- pass/fail summary against KPI thresholds

## 6. Diagnose Semantic Adapter Availability

For runtime discovery order and setup instructions, see the
[README semantic adapter setup](../../README.md#semantic-adapter-setup).

### TypeScript

If TypeScript semantic coverage is unexpectedly absent:

- verify `tsserver` exists in one of the discovery locations
- re-run indexing and inspect adapter breakdown in CLI output
- check file errors and structured logs for startup failures

### Kotlin

If Kotlin semantic coverage is unexpectedly absent:

- verify `java` is available
- verify the bridge JAR exists and is readable
- re-run indexing and inspect adapter breakdown in CLI output
- check logs for bridge startup or protocol errors

### Fallback expectation

If semantic runtimes are missing, indexing should still succeed with syntax
adapters where policy allows fallback.

## 7. Run the Main Quality Gates Locally

Format:

```bash
cargo fmt --all -- --check
```

Lint:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Tests:

```bash
cargo test --workspace --all-features
```

## 8. Inspect Regression KPIs

Fixture-based semantic regression suites:

```bash
cargo test -p adapter-semantic-typescript --test quality_regression -- --nocapture
cargo test -p adapter-semantic-kotlin --test quality_regression -- --nocapture
```

These print formatted regression reports used by the CI KPI artifact.

Indexer metrics-focused tests:

```bash
cargo test -p indexer metrics:: -- --nocapture
```

## 9. CI KPI Artifact Workflow

The CI job `rust-quality-kpi`:

- runs semantic regression suites
- captures formatted regression reports
- captures indexer metrics test output
- uploads `quality-kpi-report` as a build artifact

Use this when reviewing semantic quality changes or milestone closeout work.

## 10. Schema and Reindex Workflow

Current state:

- schema migrations are managed by the `store` crate
- schema compatibility decisions are defined in `core-model`

Operator guidance:

- if store migration succeeds and schema compatibility allows in-place upgrade,
  continue with normal indexing
- if compatibility rules require reindex, rebuild the local index from source

Recommended clean rebuild path:

1. remove or relocate the shared database (default: `~/.codeatlas/metadata.db`)
2. rerun `codeatlas index <repo-path>` for each repository
3. verify query commands against the rebuilt index

Do not manually edit SQLite schema state.

## 11. Logging and Tracing

Structured logging defaults:

- JSON logs by default
- compact logs when `CODEATLAS_LOG_FORMAT=compact`

Useful examples:

```bash
CODEATLAS_LOG=debug cargo run -p cli -- index <repo-path>
CODEATLAS_LOG_FORMAT=compact cargo run -p cli -- quality-report <repo-path>
```

OpenTelemetry export can be enabled with:

```bash
CODEATLAS_OTEL=1 cargo run -p cli -- index <repo-path>
```

## 12. Key Failure Modes

### No symbols extracted

Check:

- repo path is valid
- files are supported by current language detection and adapters
- file errors printed by the CLI

### Semantic coverage unexpectedly zero

Check:

- semantic runtime dependencies
- adapter breakdown in index / quality-report output
- startup errors in logs

### High file error count

Check:

- unsupported languages
- malformed inputs
- adapter subprocess startup failures
- resource or timeout failures

### CI quality KPI artifact empty or incomplete

Check:

- regression tests still print the formatted report under `--nocapture`
- workflow grep patterns still match the printed output
- uploaded artifact path in CI workflow is unchanged

## 13. Reference Documents

- `README.md`
- `docs/architecture/deployment-modes.md`
- `docs/specifications/rust-code-intelligence-platform-spec.md`
- `docs/workflow/github-process.md`
- `docs/benchmarks/corpus.md`

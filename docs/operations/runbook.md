# Operations Runbook

This runbook documents the key operational workflows for CodeAtlas in its
current local-first deployment mode.

It is intended to be actionable for maintainers and local operators.

## Scope

This runbook covers:

- persistent service startup and management
- repo catalog lifecycle operations
- MCP bridge setup for AI clients
- direct-store indexing and query workflows
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

- `CODEATLAS_DATA_ROOT` — override the shared storage root (default: `~/.codeatlas`)
- `CODEATLAS_PORT` — override the service port (default: `52337`)
- `CODEATLAS_HOST` — override the service bind address (default: `127.0.0.1`)
- `CODEATLAS_LOG` — set log level (e.g. `debug`, `info`)
- `CODEATLAS_LOG_FORMAT` — `json` (default) or `compact`
- `CODEATLAS_OTEL` — set to `1` to enable OpenTelemetry export
- `TSSERVER_PATH` — explicit path to `tsserver` binary
- `JAVA_HOME` — JDK location for Kotlin semantic adapter
- `KOTLIN_BRIDGE_JAR` — path to Kotlin analysis bridge JAR

## 2. Start the Persistent Service

The canonical local model is one persistent service managing multiple repos.

### Start the service

```bash
codeatlas serve
```

The service listens on `127.0.0.1:52337` by default. Override with:

```bash
codeatlas serve --port 8080
codeatlas serve --data-root /path/to/store
codeatlas serve --host 0.0.0.0  # caution: exposes to network
```

### Verify it's running

```bash
curl http://127.0.0.1:52337/health    # 200 OK
curl http://127.0.0.1:52337/status    # JSON with version, uptime, repo count
```

### Stop the service

Send SIGINT (Ctrl-C) or SIGTERM. The service shuts down gracefully.

## 3. Manage Repositories

### Add a repository

```bash
codeatlas repo add /absolute/path/to/my-app
codeatlas repo add /absolute/path/to/billing --repo-id billing-v2
codeatlas repo add /absolute/path/to/my-app --git-diff  # incremental
```

### List repositories

```bash
codeatlas repo list
```

### Check status

```bash
codeatlas repo status my-app
```

### Re-index after code changes

```bash
codeatlas repo refresh my-app
codeatlas repo refresh my-app --git-diff  # incremental
```

### Remove a repository

```bash
codeatlas repo remove my-app
```

This deletes metadata, files, symbols, and orphaned blobs.

## 4. Connect AI Clients (MCP Bridge)

The MCP bridge proxies tool calls from AI clients to the persistent service.

### Configure the client

Add to your AI client's MCP config:

```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "codeatlas",
      "args": ["mcp", "bridge"]
    }
  }
}
```

See the [README MCP Server Setup](../../README.md#mcp-server-setup) for
client-specific examples (Claude Desktop, Cursor, OpenAI Codex CLI).

### Override the service address

```bash
codeatlas mcp bridge --service-url 127.0.0.1:8080
```

Or via environment: `CODEATLAS_PORT`, `CODEATLAS_HOST`.

### Troubleshoot bridge issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| `cannot reach CodeAtlas service` | Service not running | Start with `codeatlas serve` |
| Empty tool results | Wrong `repo_id` | Use `list_repos` tool or `codeatlas repo list` |
| No stdout response | Client uses wrong framing | Bridge uses newline-delimited JSON-RPC |

## 5. Direct-Store Workflows (Legacy)

The direct-store commands still work for simple single-repo use or low-level
operations without the persistent service.

### Index a repository

```bash
codeatlas index <repo-path>
codeatlas index <repo-path> --db <db-path>
codeatlas index <repo-path> --git-diff
```

### Query the local index

```bash
codeatlas search-symbols <query> --repo <repo-id>
codeatlas get-symbol <symbol-id>
codeatlas file-outline <path> --repo <repo-id>
codeatlas file-tree --repo <repo-id>
codeatlas repo-outline --repo <repo-id>
```

### Run the direct MCP server

```bash
codeatlas mcp serve --db ~/.codeatlas/metadata.db
```

See [MCP Client Compatibility Notes](../architecture/mcp-client-compatibility.md)
for interoperability shims and rationale.

## 6. Generate a Repository Quality Report

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

## 7. Diagnose Semantic Adapter Availability

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

## 8. Run the Main Quality Gates Locally

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

## 9. Inspect Regression KPIs

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

## 10. CI KPI Artifact Workflow

The CI job `rust-quality-kpi`:

- runs semantic regression suites
- captures formatted regression reports
- captures indexer metrics test output
- uploads `quality-kpi-report` as a build artifact

Use this when reviewing semantic quality changes or milestone closeout work.

## 11. Schema and Reindex Workflow

Current state:

- schema migrations are managed by the `store` crate
- schema compatibility decisions are defined in `core-model`

Operator guidance:

- if store migration succeeds and schema compatibility allows in-place upgrade,
  continue with normal indexing
- if compatibility rules require reindex, rebuild the local index from source

Recommended clean rebuild path (service mode):

1. stop the service
2. for each repo: `codeatlas repo refresh <repo_id>`
3. if a full rebuild is needed: `codeatlas repo remove <repo_id>` then
   `codeatlas repo add <path>` for each repository
4. verify with `codeatlas repo list` and `codeatlas repo status <repo_id>`

Recommended clean rebuild path (direct-store mode):

1. remove or relocate the shared database (default: `~/.codeatlas/metadata.db`)
2. rerun `codeatlas index <repo-path>` for each repository
3. verify query commands against the rebuilt index

Do not manually edit SQLite schema state.

## 12. Logging and Tracing

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

## 13. Key Failure Modes

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

## 14. Reference Documents

- `README.md`
- `docs/architecture/deployment-modes.md`
- `docs/architecture/persistent-local-service.md`
- `docs/architecture/mcp-client-compatibility.md`
- `docs/specifications/rust-code-intelligence-platform-spec.md`
- `docs/workflow/github-process.md`
- `docs/benchmarks/corpus.md`

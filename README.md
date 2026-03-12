# CodeAtlas

CodeAtlas is a Rust-based code intelligence system for AI agents and developer
tools.

It indexes a repository once, stores a local structured index, and lets you ask
high-signal questions about symbols, files, and repository structure without
re-reading the whole codebase every time.

## Table of Contents

- [What CodeAtlas Is](#what-codeatlas-is)
- [What You Can Do With It](#what-you-can-do-with-it)
- [Quick Start](#quick-start)
- [Semantic Adapter Setup](#semantic-adapter-setup)
- [MCP Server Setup](#mcp-server-setup)
- [How To Use It With AI Today](#how-to-use-it-with-ai-today)
- [MCP Integration Shape](#mcp-integration-shape)
- [AI Usage Examples](#ai-usage-examples)
- [Current Status](#current-status)
- [Design Principles](#design-principles)
- [Roadmap](#roadmap)
- [Contributing](#contributing)

## What CodeAtlas Is

CodeAtlas gives you:

- Fast symbol search across indexed repositories.
- File and repository structure views.
- Semantic-first extraction where language-native analysis is available.
- Syntax fallback where semantic runtimes are unavailable.
- Deterministic outputs suitable for automation and AI tooling.

Today, this repository provides:

- a local CLI you can run now
- a built-in MCP server: `codeatlas mcp serve --db <path>`
- a reusable MCP library surface (`server-mcp`)
- a local-first architecture with hosted-ready boundaries

Today, this repository does **not** provide:

- a standalone hosted deployment
- HTTP/gRPC product APIs

You can use CodeAtlas immediately through the CLI or by pointing any stdio
MCP-capable AI client at `codeatlas mcp serve --db <path>`.

## What You Can Do With It

- `index`: build or update a local index for a repository
- `search-symbols`: find functions, classes, methods, types, and constants
- `get-symbol`: fetch an exact symbol by stable ID (CLI uses hyphens; MCP uses underscores, e.g. `get_symbol` / `get_symbols`)
- `file-outline`: inspect symbols in a file
- `file-tree`: inspect indexed files in a repo
- `repo-outline`: inspect repository structure and counts
- `quality-report`: inspect semantic coverage and merge quality metrics

## Quick Start

### 1. Build the CLI

```bash
cargo build -p cli
```

You can then run the binary directly:

```bash
./target/debug/codeatlas help
```

Or with Cargo:

```bash
cargo run -p cli -- help
```

### 2. Index a Repository

```bash
cargo run -p cli -- index /absolute/path/to/repo
```

Useful options:

```bash
cargo run -p cli -- index /absolute/path/to/repo --db /tmp/codeatlas.db
cargo run -p cli -- index /absolute/path/to/repo --git-diff
```

The CLI creates a local index database and content blob store. By default the
DB lives under:

```text
<repo>/.codeatlas/index.db
```

### 3. Find the Repo ID

The CLI derives `repo_id` from the indexed directory name.

Example:

- repo path: `/Users/alex/work/my-app`
- repo id: `my-app`

You will need that `repo_id` for query commands.

### 4. Query the Index

Search for symbols:

```bash
cargo run -p cli -- search-symbols greet --db /absolute/path/to/repo/.codeatlas/index.db --repo my-app
```

Search with filters:

```bash
cargo run -p cli -- search-symbols service --db /absolute/path/to/repo/.codeatlas/index.db --repo my-app --kind class --language typescript --limit 10
```

Get a symbol by ID:

```bash
cargo run -p cli -- get-symbol 'src/lib.rs::greet#function' --db /absolute/path/to/repo/.codeatlas/index.db
```

Get a file outline:

```bash
cargo run -p cli -- file-outline src/lib.rs --db /absolute/path/to/repo/.codeatlas/index.db --repo my-app
```

Get a file tree:

```bash
cargo run -p cli -- file-tree --db /absolute/path/to/repo/.codeatlas/index.db --repo my-app
```

Get a repository outline:

```bash
cargo run -p cli -- repo-outline --db /absolute/path/to/repo/.codeatlas/index.db --repo my-app
```

Generate a quality report:

```bash
cargo run -p cli -- quality-report /absolute/path/to/repo
```

## Semantic Adapter Setup

Semantic coverage is optional. Indexing still works without it. For detailed
diagnosis steps, see the [operations runbook](docs/operations/runbook.md#5-diagnose-semantic-adapter-availability).

### TypeScript semantic adapter

CodeAtlas looks for `tsserver` in this order:

1. `TSSERVER_PATH`
2. `node_modules/.bin/tsserver`
3. system `PATH`

### Kotlin semantic adapter

CodeAtlas looks for Kotlin runtime dependencies in this order:

1. `JAVA_HOME/bin/java`
2. system `PATH`
3. `KOTLIN_BRIDGE_JAR`
4. repo-local `.codeatlas/kotlin-bridge.jar`

If those runtimes are missing, CodeAtlas falls back to syntax adapters where
policy allows.

## MCP Server Setup

CodeAtlas includes a built-in MCP server that works with any stdio MCP client.

### Prerequisites

1. Build or install `codeatlas` (see [Quick Start](#quick-start)).
2. Index a repository so an index database exists.

### Launch the MCP server

```bash
codeatlas mcp serve --db /path/to/repo/.codeatlas/index.db
```

The server communicates over stdio using newline-delimited JSON-RPC 2.0
(MCP spec 2025-11-25). All diagnostics go to stderr; stdout is reserved for
protocol messages only.

### Client configuration

#### Claude Desktop

Add to your Claude Desktop MCP config (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "/path/to/codeatlas",
      "args": ["mcp", "serve", "--db", "/path/to/repo/.codeatlas/index.db"]
    }
  }
}
```

#### Cursor

Add to your Cursor MCP settings (`.cursor/mcp.json` in your project or global
config):

```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "/path/to/codeatlas",
      "args": ["mcp", "serve", "--db", "/path/to/repo/.codeatlas/index.db"]
    }
  }
}
```

#### OpenAI Codex CLI

Create or edit `.codex/config.json` in your project root:

```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "/path/to/codeatlas",
      "args": ["mcp", "serve", "--db", "/path/to/repo/.codeatlas/index.db"]
    }
  }
}
```

#### Generic stdio MCP client

Any client that speaks newline-delimited JSON-RPC over stdio can use CodeAtlas.
Configure it to spawn:

```
/path/to/codeatlas mcp serve --db /path/to/repo/.codeatlas/index.db
```

### Available tools

The MCP server exposes these tools via `tools/list`, each with full JSON Schema
input definitions:

| Tool | Description |
|------|-------------|
| `search_symbols` | Search for symbols by name with optional filters |
| `get_symbol` | Get a symbol by its unique ID |
| `get_symbols` | Get multiple symbols by their IDs |
| `get_file_outline` | List symbols defined in a file |
| `get_file_content` | Get the content of an indexed file |
| `get_file_tree` | List files in a repository or subtree |
| `get_repo_outline` | Show repository structure and file summary |
| `search_text` | Search for text patterns across indexed files |

Repository-scoped tools accept `repo_id` as a parameter. The `repo_id` is
derived from the indexed directory name (e.g., indexing `/home/user/my-app`
produces `repo_id` `my-app`).

### Verify the server starts

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | codeatlas mcp serve --db /path/to/repo/.codeatlas/index.db
```

You should see a JSON response containing `"protocolVersion":"2025-11-25"` and
`"serverInfo":{"name":"codeatlas",...}`.

### Troubleshooting

**"database not found"** — The `--db` path does not exist. Run
`codeatlas index <repo-path>` first to create the index, then use the
generated database path (default: `<repo>/.codeatlas/index.db`).

**"database is not readable"** — The file exists but cannot be read. Check
file permissions (`ls -la` on the DB path).

**"failed to open database"** — The file exists and is readable but is not a
valid CodeAtlas index (possibly corrupt or not a SQLite database). Re-run
`codeatlas index <repo-path>` to rebuild.

**"database path is a directory"** — The `--db` path points to a directory
instead of a file. Point it at the `.db` file inside the directory.

**No response on stdout** — Ensure the client uses newline-delimited JSON-RPC,
not Content-Length framing. CodeAtlas will reject Content-Length headers with a
clear error.

**Empty tool results** — Verify the `repo_id` matches the indexed repository.
The `repo_id` is derived from the directory name used during indexing.

### What is not supported

- HTTP, gRPC, or WebSocket transports (stdio only)
- Content-Length framed MCP (2024-11-05 transport)
- Authentication, tenancy, or multi-user access
- Remote/hosted serving

## How To Use It With AI Today

### Option 1: MCP server (recommended)

The simplest path is to use the built-in MCP server. See
[MCP Server Setup](#mcp-server-setup) above.

### Option 2: Use the CLI as a retrieval tool in your agent workflow

Typical loop:

1. Index the repository once.
2. Let the agent call `codeatlas` CLI commands when it needs structure.
3. Feed only the relevant output back into the agent.

Example shell commands an agent or wrapper can call:

```bash
codeatlas search-symbols AuthService --db /repo/.codeatlas/index.db --repo my-app
codeatlas file-outline src/server.ts --db /repo/.codeatlas/index.db --repo my-app
codeatlas repo-outline --db /repo/.codeatlas/index.db --repo my-app
```

## MCP Integration Shape

The MCP server returns tool results as structured envelopes with:

- `status` — `"success"` or `"error"`
- `payload` — the tool's JSON result
- `error` — structured error with code, message, and retryable flag
- `_meta` — envelope metadata

The `_meta` payload includes:

- `timing_ms` — wall-clock time for the tool call
- `truncated` — whether results were capped by a limit
- `quality_stats` — semantic/syntax quality mix of returned results
- `index_version` — schema version of the index that served the query

This makes it suitable for agents that need structured retrieval, stable tool
behavior, and quality provenance.

## AI Usage Examples

### Example: narrow a refactor target

1. Run:

```bash
codeatlas search-symbols PaymentService --db /repo/.codeatlas/index.db --repo billing
```

2. Pick the exact symbol ID from the results.
3. Run:

```bash
codeatlas get-symbol 'src/payment/service.ts::PaymentService#class' --db /repo/.codeatlas/index.db
```

4. Ask the agent to reason about only that class and its file outline instead of
   the entire repository.

### Example: inspect a suspicious file before editing

```bash
codeatlas file-outline src/auth/session.rs --db /repo/.codeatlas/index.db --repo my-app
```

Use that output to ask the agent:

- which functions own session invalidation
- where to patch auth behavior
- whether the file contains related helper methods

### Example: verify semantic coverage before trusting structural results

```bash
codeatlas quality-report /absolute/path/to/repo
```

Use that output to decide whether the agent should trust semantic results or
fall back to broader file reads.

## Current Status

Milestones M0-M9 are complete:

- M0-M4: governance, core model, discovery/adapters, storage, and indexing pipeline.
- M5: query engine and deterministic ranking.
- M6: MCP server contracts and local CLI interface.
- M7: incremental indexing, git-diff acceleration, and determinism regression coverage.
- M8: OpenTelemetry tracing, structured logging with redaction, security regression suites, and benchmark threshold enforcement.
- M9: TypeScript and Kotlin semantic adapters, confidence-aware syntax+semantic merge, semantic quality regression gating, and semantic coverage/win-rate metrics.

### Workspace crates

| Crate | Purpose | Status |
|-------|---------|--------|
| `core-model` | Canonical Symbol/File/Repo schemas, symbol ID construction, schema versioning | Complete |
| `repo-walker` | Repository traversal with gitignore/security filters, language detection, structured logging | Complete |
| `adapter-api` | `LanguageAdapter` and `AdapterRouter` traits, routing policy, contract test harness | Complete |
| `adapter-syntax-treesitter` | Tree-sitter-based syntax extraction (Rust supported, table-driven for extensibility) | Complete |
| `adapter-semantic-typescript` | `tsserver`-backed semantic extraction, runtime lifecycle, mapping, and regression coverage | Complete |
| `adapter-semantic-kotlin` | JVM bridge-backed semantic extraction, runtime lifecycle, mapping, and regression coverage | Complete |
| `store` | SQLite metadata persistence with versioned migrations, typed CRUD for repos/files/symbols | Complete |
| `indexer` | End-to-end indexing pipeline (discovery -> parse -> enrich -> persist) | Complete |
| `query-engine` | Symbol/text search, symbol lookup, file/repo outline retrieval | Complete |
| `server-mcp` | MCP tool registry, structured response/error envelopes, integration + E2E tests | Complete |
| `cli` | Local commands, MCP stdio server (`codeatlas mcp serve`), indexing, search/get symbol, file/repo outline | Complete |

### Infrastructure

- Product and implementation specification in `docs/specifications/`.
- Issue-driven execution plan with one-PR-per-issue policy.
- Governance and contribution workflow docs.
- GitHub Actions CI quality gates for PRs and pushes to `master` (fmt, clippy, tests, build, docs, MSRV check).
- Serde compatibility fixtures and snapshot tests for schema forward-compatibility.
- Adapter contract test harness for preventing adapter drift across implementations.
- Semantic regression harness with fixture-based KPI thresholds for TypeScript and Kotlin.
- OpenTelemetry span instrumentation across indexing, query, and MCP request flows.
- Structured CLI logging with sensitive-field redaction for local and machine-readable output.
- Security regression coverage for malicious inputs, traversal/symlink escape, malformed files, and resource limits.
- Benchmark and threshold coverage in CI for discovery, indexing, and query latency regressions.

### What does not exist yet

- Watcher/local file-system triggered update mode.
- Semantic adapters beyond the current TypeScript and Kotlin implementations.
- Hosted/server API surface (HTTP/gRPC), auth, quotas, and multi-tenant controls.
- Production observability dashboards and hosted telemetry/export integrations beyond the local CLI baseline.

### Semantic Adapter Runtime Discovery

Production CLI indexing will register semantic adapters when their local runtime
dependencies are available, and otherwise fall back to syntax-only parsing.

- TypeScript semantic adapter:
  - `TSSERVER_PATH`
  - repo-local `node_modules/.bin/tsserver`
  - system `PATH`
- Kotlin semantic adapter:
  - `JAVA_HOME/bin/java` or system `PATH`
  - `KOTLIN_BRIDGE_JAR`
  - repo-local `.codeatlas/kotlin-bridge.jar`

If those dependencies are not present, indexing still succeeds with syntax
adapters and the router keeps the semantic-preferred policy behavior where
available.

## Design Principles

- Semantic-first, syntax-fallback intelligence.
- Local-first trust model with hosted-ready architecture.
- Security by design (treat indexed code as sensitive, untrusted input).
- Strong determinism and stable API/tool behavior.
- Observability and quality metrics as first-class requirements.

## Roadmap

Planning artifacts:

- `docs/architecture/rust-code-intelligence-plan.md`
- `docs/architecture/deployment-modes.md`
- `docs/operations/runbook.md`
- `docs/planning/issue-backlog.md`
- `docs/planning/post-v1-roadmap.md`
- `docs/workflow/github-process.md`
- `docs/engineering-principles.md`

Canonical specification:

- `docs/specifications/rust-code-intelligence-platform-spec.md`

## Contributing

Contributions follow strict quality gates and issue/PR discipline.

Start here:

- `CONTRIBUTING.md`

## License

License is not defined yet.

Add a `LICENSE` file before first public release.

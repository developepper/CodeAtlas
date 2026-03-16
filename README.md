# CodeAtlas

CodeAtlas is a Rust-based code intelligence system for AI agents and developer
tools.

It indexes a repository once, stores a local structured index, and lets you ask
high-signal questions about symbols, files, and repository structure without
re-reading the whole codebase every time.

## Table of Contents

- [What CodeAtlas Is](#what-codeatlas-is)
- [What You Can Do With It](#what-you-can-do-with-it)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Semantic Adapter Setup](#semantic-adapter-setup)
- [MCP Server Setup](#mcp-server-setup)
- [How To Use It With AI Today](#how-to-use-it-with-ai-today)
- [MCP Integration Shape](#mcp-integration-shape)
- [AI Usage Examples](#ai-usage-examples)
- [Service Architecture](#service-architecture)
- [Current Status](#current-status)
- [Design Principles](#design-principles)
- [Roadmap](#roadmap)
- [Contributing](#contributing)

## What CodeAtlas Is

CodeAtlas gives you:

- Fast symbol search across indexed repositories.
- File and repository structure views.
- File content retrieval from the local index.
- Syntax indexing baseline with semantic enrichment where language-native
  analysis is available.
- File-level indexing baseline for recognized languages even without symbol adapters.
- Deterministic outputs suitable for automation and AI tooling.

Today, this repository provides:

- a persistent local service: `codeatlas serve`
- a multi-repo catalog with lifecycle operations: `codeatlas repo add/list/status/refresh/remove`
- an MCP bridge for AI clients: `codeatlas mcp bridge`
- a direct MCP server for simple single-repo use: `codeatlas mcp serve`
- a local CLI for indexing and querying
- a local-first architecture with hosted-ready boundaries

Today, this repository does **not** provide:

- a standalone hosted deployment
- auth, tenancy, or multi-user controls
- Docker packaging (deferred follow-up)

The canonical local usage model is one persistent service managing multiple
repositories. AI clients connect through the MCP bridge.

## What You Can Do With It

### Service and repo management

- `serve`: start the persistent local HTTP service
- `repo add <path>`: register and index a repository
- `repo list`: list all registered repositories with status
- `repo status <repo_id>`: show detailed status for a repository
- `repo refresh <repo_id>`: re-index a registered repository
- `repo remove <repo_id>`: de-register a repository and clean up data

### Querying

- `search-symbols`: find functions, classes, methods, types, and constants
- `get-symbol`: fetch an exact symbol by stable ID (CLI uses hyphens; MCP uses underscores, e.g. `get_symbol` / `get_symbols`)
- `file-outline`: inspect symbols in a file
- `file-tree`: inspect indexed files in a repo
- `repo-outline`: inspect repository structure and counts

### AI integration

- `mcp bridge`: start the MCP bridge to the persistent service (recommended)
- `mcp serve`: start a direct stdio MCP server (simple single-repo use)

### Other

- `index`: build or update a local index (lower-level; prefer `repo add`)
- `quality-report`: inspect semantic coverage and merge quality metrics

## Installation

### Option 1: Install from source with Cargo (recommended for v1)

Requires the Rust toolchain (1.81+).

```bash
cargo install --git https://github.com/developepper/CodeAtlas.git --bin codeatlas
```

This builds and installs the `codeatlas` binary to `~/.cargo/bin/`. Make sure
that directory is in your `PATH`.

After installation, verify it works:

```bash
codeatlas help
```

### Option 2: Build from a local clone

```bash
git clone https://github.com/developepper/CodeAtlas.git
cd CodeAtlas
cargo build --release -p cli
```

The binary is at `./target/release/codeatlas`. Copy it to a directory in your
`PATH` or run it directly.

### What is not yet supported

- Prebuilt GitHub Release binaries
- Homebrew formula
- Platform-specific installers (`.deb`, `.rpm`, `.msi`)
- Docker images

These may be added in future releases based on demand.

## Quick Start

### 1. Install CodeAtlas

See [Installation](#installation) above. The examples below assume `codeatlas`
is in your `PATH`.

### 2. Start the service and add repositories

```bash
# Start the persistent service (runs in foreground).
codeatlas serve &

# Add repositories to the catalog.
codeatlas repo add /absolute/path/to/my-app
codeatlas repo add /absolute/path/to/billing

# Check what's registered.
codeatlas repo list
```

The service stores data in `~/.codeatlas/` by default (override with
`--data-root <path>` or `CODEATLAS_DATA_ROOT`). The `repo_id` is derived from
the directory name — indexing `/Users/alex/work/my-app` produces `repo_id`
`my-app`.

### 3. Connect an AI client

Configure your AI client to use the MCP bridge:

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

That's it. The bridge connects to the running service and exposes all indexed
repositories through the standard MCP tool interface. See
[MCP Server Setup](#mcp-server-setup) for client-specific examples.

### 4. Query from the CLI

All query commands default to the shared store at `~/.codeatlas/metadata.db`.
Use `--db <path>` to override.

```bash
codeatlas search-symbols greet --repo my-app
codeatlas get-symbol 'my-app//src/lib.rs::greet#function'
codeatlas file-outline src/lib.rs --repo my-app
codeatlas file-tree --repo my-app
codeatlas repo-outline --repo my-app
```

### 5. Manage repositories

```bash
# Re-index after code changes.
codeatlas repo refresh my-app

# Check status.
codeatlas repo status my-app

# Remove a repo and clean up its data.
codeatlas repo remove my-app
```

## Semantic Adapter Setup

Semantic coverage is optional. Indexing still works without it — recognized
files are always indexed at the file level (file tree, file content retrieval,
repo outline) even when no symbol adapter is available. For detailed diagnosis
steps, see the [operations runbook](docs/operations/runbook.md#5-diagnose-semantic-adapter-availability).

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

CodeAtlas provides two MCP integration paths:

1. **MCP bridge** (recommended) — a thin stdio process that proxies tool calls
   to the persistent CodeAtlas service. One service, many repos, one client
   config.
2. **Direct MCP server** — a standalone stdio server for simple single-repo
   workflows. No service needed.

Both speak newline-delimited JSON-RPC 2.0 (MCP spec 2025-11-25). Compatibility
notes and interoperability shims are documented in
[docs/architecture/mcp-client-compatibility.md](docs/architecture/mcp-client-compatibility.md).

### Option 1: MCP bridge (recommended)

Prerequisites:

1. Install `codeatlas` (see [Installation](#installation)).
2. Start the service: `codeatlas serve`
3. Add at least one repo: `codeatlas repo add /path/to/repo`

#### Client configuration

##### Claude Desktop

Add to your Claude Desktop MCP config (`claude_desktop_config.json`):

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

##### Cursor

Add to your Cursor MCP settings (`.cursor/mcp.json`):

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

##### OpenAI Codex CLI

Create or edit `.codex/config.json`:

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

##### Generic stdio MCP client

Configure it to spawn:

```
codeatlas mcp bridge
```

The bridge connects to the service at `127.0.0.1:52337` by default. Override
with `--service-url <host:port>` or the `CODEATLAS_PORT`/`CODEATLAS_HOST`
environment variables.

### Option 2: Direct MCP server

For simple single-repo use without running the persistent service:

```bash
codeatlas mcp serve --db ~/.codeatlas/metadata.db
```

Client configuration is the same as above but uses `["mcp", "serve"]` (or
`["mcp", "serve", "--db", "/path/to/metadata.db"]`) instead of
`["mcp", "bridge"]`.

### Available tools

Both the bridge and direct server expose these tools via `tools/list`:

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
| `list_repos` | List all indexed repositories with status |
| `get_repo_status` | Get detailed status for a specific repository |

Repository-scoped tools accept `repo_id` as a parameter. Symbol IDs include
the repo prefix (e.g., `my-app//src/lib.rs::Config#class`). The `repo_id` is
derived from the indexed directory name (e.g., indexing `/home/user/my-app`
produces `repo_id` `my-app`).

### Troubleshooting

**"cannot reach CodeAtlas service"** (bridge) — The service is not running.
Start it with `codeatlas serve`.

**"database not found"** (direct server) — The `--db` path does not exist.
Run `codeatlas repo add <path>` or `codeatlas index <path>` first.

**Empty tool results** — Verify the `repo_id` matches an indexed repository.
Use `codeatlas repo list` or the `list_repos` tool to see available repos.

**No response on stdout** — Ensure the client uses newline-delimited JSON-RPC,
not Content-Length framing.

### What is not supported

- Content-Length framed MCP (2024-11-05 transport)
- Authentication, tenancy, or multi-user access
- Remote/hosted serving
- The service HTTP API (`/tools/call`, `/repos`, etc.) is internal to the
  local machine. AI clients should use the MCP bridge, not the HTTP endpoints
  directly.

## How To Use It With AI Today

### Option 1: MCP bridge with persistent service (recommended)

Start the service, add your repos, and configure your AI client to use the
bridge. See [Quick Start](#quick-start) and [MCP Server Setup](#mcp-server-setup).

### Option 2: Use the CLI as a retrieval tool in your agent workflow

Typical loop:

1. Index the repository once.
2. Let the agent call `codeatlas` CLI commands when it needs structure.
3. Feed only the relevant output back into the agent.

Example shell commands an agent or wrapper can call:

```bash
codeatlas search-symbols AuthService --repo my-app
codeatlas file-outline src/server.ts --repo my-app
codeatlas repo-outline --repo my-app
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
codeatlas search-symbols PaymentService --repo billing
```

2. Pick the exact symbol ID from the results (symbol IDs include the repo
   prefix, e.g. `billing//src/payment/service.ts::PaymentService#class`).
3. Run:

```bash
codeatlas get-symbol 'billing//src/payment/service.ts::PaymentService#class'
```

4. Ask the agent to reason about only that class and its file outline instead of
   the entire repository.

### Example: inspect a suspicious file before editing

```bash
codeatlas file-outline src/auth/session.rs --repo my-app
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

## Service Architecture

The canonical local deployment is one persistent CodeAtlas service managing
multiple repositories:

```
AI Client (Claude, Cursor)
    |  stdio MCP
MCP Bridge (codeatlas mcp bridge)
    |  HTTP (localhost)
CodeAtlas Service (codeatlas serve)
    |
Shared Store (~/.codeatlas/)
    ├── metadata.db
    ├── blobs/
    ├── repo: my-app
    ├── repo: billing
    └── repo: shared-lib
```

The service listens on `127.0.0.1:52337` by default and exposes:

- `GET /health` — health check
- `GET /status` — service metadata (version, uptime, repo count)
- `GET /repos` — list all repositories
- `GET /repos/{repo_id}` — repository details
- `DELETE /repos/{repo_id}` — remove a repository
- `POST /tools/call` — execute a query tool

The direct-store commands (`codeatlas index`, `codeatlas mcp serve --db`,
`codeatlas search-symbols --repo`, etc.) continue to work for low-level
workflows. The service model is the recommended path for daily multi-repo use.

For architecture decisions and the relationship to future hosted deployment,
see `docs/architecture/persistent-local-service.md`.

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
| `service` | Persistent local HTTP service runtime (`codeatlas serve`), repo catalog and query APIs | Complete |
| `cli` | Local commands, MCP stdio server, MCP bridge, repo lifecycle, service startup, indexing, querying | Complete |

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

### Language coverage

CodeAtlas recognizes many languages during discovery (Rust, TypeScript,
JavaScript, Kotlin, Java, Python, PHP, Go, Ruby, C, C++, C#, Swift, Shell,
JSON, YAML, TOML, Markdown, SQL, Dockerfile). The indexing depth depends on
adapter availability:

| Level | Languages | What works |
|-------|-----------|------------|
| Semantic + syntax | TypeScript, Kotlin (when runtimes are available) | Symbol search, file outline, file tree, file content, repo outline |
| Syntax only | Rust | Symbol search, file outline, file tree, file content, repo outline |
| File-level only | All other recognized languages | File tree, file content, repo outline (zero symbols) |
| Not indexed | Unrecognized files (`Language::Unknown`) | Excluded at discovery |

File-level indexing is the baseline — every recognized file gets a file record,
stored content blob, and appears in file tree and repo outline queries. Symbol
extraction is additive on top of that baseline when adapters are available.

### Long-term architecture direction

The current capability table reflects the product as it exists today, not the
intended end state.

CodeAtlas is now explicitly moving toward:

- file-level persistence as the universal floor for recognized files
- syntax indexing as the default baseline for most recognized code languages
- semantic indexing as an enrichment layer on top of syntax
- file-only indexing as the explicit last fallback rather than the common case

This project is still early enough that clean long-term architecture is favored
over preserving awkward intermediate interfaces. If the indexing model, schema,
or crate boundaries need to be refactored to support that direction, those
refactors are in scope.

The planning artifact for that next major architecture initiative is:
`docs/planning/universal-syntax-indexing.md`.

### What does not exist yet

- Watcher/local file-system triggered update mode.
- Semantic adapters beyond the current TypeScript and Kotlin implementations.
- Syntax adapters beyond Rust (other recognized languages are file-level only).
- Docker packaging for the persistent service.
- Hosted auth, quotas, and multi-tenant controls.
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

If those dependencies are not present, indexing still succeeds. Languages with
syntax adapters fall back to syntax-only extraction. Recognized languages
without any adapter are still indexed at the file level — file tree, file
content retrieval, and repo outline remain useful even with zero symbols.

## Design Principles

- Broad syntax baseline with semantic enrichment.
- Local-first trust model with hosted-ready architecture.
- Security by design (treat indexed code as sensitive, untrusted input).
- Strong determinism and stable API/tool behavior.
- Observability and quality metrics as first-class requirements.
- Favor clean long-term architecture over backward compatibility while the
  project is still early.

## Roadmap

Planning artifacts:

- `docs/architecture/rust-code-intelligence-plan.md`
- `docs/architecture/deployment-modes.md`
- `docs/operations/runbook.md`
- `docs/planning/issue-backlog.md`
- `docs/planning/post-v1-roadmap.md`
- `docs/planning/universal-syntax-indexing.md`
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

# CodeAtlas

CodeAtlas is a Rust-based polyglot code intelligence platform designed for AI agents and developer tools.

It indexes repositories once, then returns precise symbol- and structure-level answers so tools do not need to read entire codebases for every question.

## What CodeAtlas Is

CodeAtlas is intended to provide:

- Fast symbol search across many programming languages.
- File and repository structural exploration (outlines, trees, symbol maps).
- Semantic-first results where language-native analysis is available.
- Reliable syntax fallback where semantic adapters are unavailable.
- Deterministic outputs suitable for automation and AI tooling.

Primary interface target:

- MCP server (first-class)
- Optional HTTP/gRPC APIs (planned)

## What It Can Do (Target Capabilities)

- `search_symbols`: find relevant functions, classes, methods, types, constants.
- `get_symbol` / `get_symbols`: fetch exact symbol details with locations and signatures.
- `get_file_outline`: retrieve structural map of a file.
- `get_file_tree` / `get_repo_outline`: navigate repository shape quickly.
- `search_text`: fallback full-text retrieval when symbol lookup is insufficient.

## Current Status

Milestones M0-M8 are complete:

- M0-M4: governance, core model, discovery/adapters, storage, and indexing pipeline.
- M5: query engine and deterministic ranking.
- M6: MCP server contracts and local CLI interface.
- M7: incremental indexing, git-diff acceleration, and determinism regression coverage.
- M8: OpenTelemetry tracing, structured logging with redaction, security regression suites, and benchmark threshold enforcement.

### Workspace crates

| Crate | Purpose | Status |
|-------|---------|--------|
| `core-model` | Canonical Symbol/File/Repo schemas, symbol ID construction, schema versioning | Complete |
| `repo-walker` | Repository traversal with gitignore/security filters, language detection, structured logging | Complete |
| `adapter-api` | `LanguageAdapter` and `AdapterRouter` traits, routing policy, contract test harness | Complete |
| `adapter-syntax-treesitter` | Tree-sitter-based syntax extraction (Rust supported, table-driven for extensibility) | Complete |
| `store` | SQLite metadata persistence with versioned migrations, typed CRUD for repos/files/symbols | Complete |
| `indexer` | End-to-end indexing pipeline (discovery -> parse -> enrich -> persist) | Complete |
| `query-engine` | Symbol/text search, symbol lookup, file/repo outline retrieval | Complete |
| `server-mcp` | MCP tool registry, structured response/error envelopes, integration + E2E tests | Complete |
| `cli` | Local commands for indexing, search/get symbol, file/repo outline navigation | Complete |

### Infrastructure

- Product and implementation specification in `docs/specifications/`.
- Issue-driven execution plan with one-PR-per-issue policy.
- Governance and contribution workflow docs.
- GitHub Actions CI quality gates for PRs and pushes to `master` (fmt, clippy, tests, build, docs, MSRV check).
- Serde compatibility fixtures and snapshot tests for schema forward-compatibility.
- Adapter contract test harness for preventing adapter drift across implementations.
- OpenTelemetry span instrumentation across indexing, query, and MCP request flows.
- Structured CLI logging with sensitive-field redaction for local and machine-readable output.
- Security regression coverage for malicious inputs, traversal/symlink escape, malformed files, and resource limits.
- Benchmark and threshold coverage in CI for discovery, indexing, and query latency regressions.

### What does not exist yet

- Watcher/local file-system triggered update mode.
- Semantic adapter implementations beyond syntax baseline.
- Hosted/server API surface (HTTP/gRPC), auth, quotas, and multi-tenant controls.
- Production observability dashboards and hosted telemetry/export integrations beyond the local CLI baseline.

## Design Principles

- Semantic-first, syntax-fallback intelligence.
- Local-first trust model with hosted-ready architecture.
- Security by design (treat indexed code as sensitive, untrusted input).
- Strong determinism and stable API/tool behavior.
- Observability and quality metrics as first-class requirements.

## Roadmap

Planning artifacts:

- `docs/architecture/rust-code-intelligence-plan.md`
- `docs/planning/issue-backlog.md`
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

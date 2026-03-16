# Rust Polyglot Code Intelligence Platform - Complete Build Specification

Status: Active baseline specification (implementation in progress)  
Project codename: `CodeAtlas`  
Primary implementation language: Rust  
Primary interface: MCP server + optional HTTP/gRPC API

## 1. Purpose

This document defines a full, implementation-ready specification for a new Rust-based code intelligence platform that:

- Works across many programming languages.
- Uses an adapter architecture (syntax + semantic adapters per language).
- Provides high-quality symbol retrieval/search for AI agents and developer tools.
- Supports local-first, hybrid, and fully hosted deployment models.
- Improves on common baseline implementations in quality, security, observability, and scalability.

This spec is self-contained and should be sufficient to start implementation planning without referencing any prior codebase.

## 2. Product Definition

## 2.1 Core Problem

AI agents and developer tooling often read too much code to answer small questions. This wastes tokens, latency, and cost.

## 2.2 Product Promise

Index once, retrieve precisely:

- Symbol-level retrieval (functions, classes, methods, types, constants).
- File-level and repo-level structural exploration.
- Full-text fallback when symbols are not enough.
- Confidence-aware results that prefer semantic intelligence where available.

## 2.3 Primary Users

- AI coding agents (MCP clients, IDE assistants).
- Engineering teams exploring large or unfamiliar codebases.
- Platform teams needing centrally managed code intelligence.

## 2.4 Non-Goals (Initial Scope)

- No code execution from indexed repositories.
- No full refactoring engine in v1.
- No source control hosting replacement.

## 3. Guiding Principles

1. Semantic-first where possible, syntax fallback everywhere.
2. Local-first trust model with optional hosted expansion.
3. Deterministic, stable tool outputs for agents.
4. Multi-tenant security by design (if hosted).
5. Measure quality, latency, and cost continuously.

## 4. High-Level Architecture

## 4.1 Major Subsystems

1. Ingestion and Discovery
- Walk local repos or fetch remote repos via connector.
- Apply ignore rules, safety rules, and file caps.

2. Adapter Engine
- Routes each file/language to:
    - Syntax adapter (tree-sitter baseline), and/or
    - Semantic adapter (language-native analysis/LSP/compiler).

3. Index Pipeline
- Normalize extracted symbols into a unified schema.
- Generate file summaries and metadata.
- Persist index and content references.

4. Query Engine
- Symbol search (ranked).
- Symbol retrieval by ID.
- File outline/tree/repo outline.
- Text search fallback.

5. Serving Interfaces
- MCP tool server.
- Optional HTTP/gRPC APIs.

6. Operations Plane
- AuthN/AuthZ, quotas, telemetry, billing hooks, audit logs.

## 4.2 Deployment Modes

Mode A: Local-only
- Indexing and query on user machine.
- No code leaves machine.

Mode B: Hybrid
- Indexing local or in customer VPC.
- Selected metadata/artifacts synced to managed service.

Mode C: Fully hosted SaaS
- Service fetches repos, builds indexes, serves team queries.
- Requires full multi-tenant controls and compliance posture.

## 5. Language Intelligence Model

## 5.1 Adapter Types

1. Syntax Adapter
- Tree-sitter-based extraction.
- Fast, broad language support.
- Lower semantic certainty.

2. Semantic Adapter
- Language-native analyzers/LSP/compiler APIs.
- Better type/call resolution and correctness.
- Heavier runtime and operational complexity.

## 5.2 Adapter Selection Policy

Per language, route by policy:

- `semantic_required`: fail if semantic adapter unavailable.
- `semantic_preferred`: use semantic when available, fallback to syntax.
- `syntax_only`: syntax adapter only.

Default:
- Kotlin/Java (Android): semantic_preferred (Kotlin/Java language servers or compiler-backed).
- TypeScript: semantic_preferred (tsserver/TypeScript compiler).
- PHP: semantic_preferred (php-parser + static analysis/LSP enrichment).
- Other languages: syntax_only until semantic adapter is implemented.

## 5.3 Unified Capability Flags

Each indexed symbol stores:

- `quality_level`: `semantic` | `syntax`
- `confidence_score`: 0.0..1.0
- `source_adapter`: adapter ID/version
- `derived_features`: optional semantic enrichments (type refs, call refs, container refs)

## 6. Unified Data Model

## 6.1 Symbol Schema (Canonical)

Required fields:

- `id`: stable ID (`{file}::{qualified_name}#{kind}`)
- `repo_id`
- `file_path`
- `language`
- `kind` (`function`, `class`, `method`, `type`, `constant`, etc.)
- `name`
- `qualified_name`
- `signature`
- `start_line`, `end_line`
- `start_byte`, `byte_length`
- `content_hash`
- `quality_level`
- `confidence_score`
- `indexed_at`

Optional fields:

- `docstring`
- `summary`
- `parent_symbol_id`
- `keywords`
- `decorators_or_attributes`
- `semantic_refs` (types/calls/imports)

## 6.2 File Schema

- `repo_id`
- `file_path`
- `language`
- `file_hash`
- `summary`
- `symbol_count`
- `quality_mix` (% semantic vs syntax)
- `updated_at`

## 6.3 Repo Schema

- `repo_id`
- `display_name`
- `source_root` (or remote URL)
- `indexed_at`
- `index_version`
- `language_counts`
- `file_count`
- `symbol_count`
- `git_head` (when available)

## 6.4 Storage Layout

Use a split design:

1. Metadata store (SQLite/Postgres)
- Repos, files, symbols, indexes, adapter runs, audit tables.

2. Content store (content-addressed blob store)
- Raw file snapshots keyed by hash.
- Optional encrypted object storage in hosted mode.

3. Search acceleration indexes
- Inverted text index (e.g., Tantivy).
- Optional vector/embedding index (future, not required v1).

## 7. Rust Architecture

## 7.1 Crate Structure (Proposed)

- `core-model`: schemas, IDs, validation, serialization.
- `syntax-platform`: tree-sitter grammar registry, `SyntaxBackend` trait, language modules.
- `semantic-api`: `SemanticBackend` trait and shared semantic types.
- `semantic-*`: per-language semantic adapters.
- `indexer`: ingestion + pipeline orchestration.
- `store`: metadata/content/search persistence.
- `query-engine`: ranking, filtering, retrieval.
- `server-mcp`: MCP tool server.
- `server-api`: optional HTTP/gRPC server.
- `ops`: auth, quotas, telemetry, billing integration.
- `cli`: admin and local commands.

## 7.2 Core Traits (Conceptual)

```rust
pub trait LanguageAdapter {
    fn adapter_id(&self) -> &'static str;
    fn language(&self) -> &'static str;
    fn capabilities(&self) -> AdapterCapabilities;
    fn index_file(&self, ctx: &IndexContext, file: &SourceFile) -> Result<AdapterOutput>;
}

pub trait AdapterRouter {
    fn select(&self, language: &str, policy: AdapterPolicy) -> Vec<Box<dyn LanguageAdapter>>;
}

pub trait QueryRanker {
    fn rank(&self, query: &SearchQuery, candidates: Vec<SymbolRecord>) -> Vec<ScoredSymbol>;
}
```

## 7.3 Plugin Model

Initial implementation: in-process adapters compiled in Rust workspace.  
Future extension: out-of-process adapters via gRPC/WASM plugin boundary for isolation.

## 8. Indexing Pipeline

## 8.1 Discovery Stage

- Inputs: local path, git URL, connector.
- Resolve ignore patterns (`.gitignore`, user-defined ignore).
- Security filters:
    - traversal/symlink protection
    - secret-file exclusion policy
    - binary file detection
    - max file size and max file count

## 8.2 Parse Stage

For each file:

1. Determine language.
2. Select adapters by policy.
3. Execute adapters with timeout/resource budget.
4. Merge output:
- deduplicate symbols
- keep higher-confidence conflicting records
- preserve provenance

## 8.3 Enrichment Stage

- Heuristic file summary (always available).
- Optional LLM summary pass (configurable, default off in enterprise mode).
- Keyword extraction and normalized searchable fields.

## 8.4 Persistence Stage

- Atomic index commit (staging -> swap).
- Update symbol/file/repo metadata.
- Write content blobs.
- Emit indexing telemetry and audit event.

## 8.5 Incremental Indexing

Change detection strategy:

- Compute file hash and compare against prior hash map.
- Optional git-diff accelerated mode.
- Local watcher mode for near-real-time updates.

On incremental update:

- Re-index changed/new files only.
- Remove deleted file symbols.
- Recompute affected aggregates.

## 9. Query and Ranking Design

## 9.1 Query Types

1. `search_symbols`
2. `get_symbol`
3. `get_symbols`
4. `get_file_outline`
5. `get_file_content`
6. `get_file_tree`
7. `get_repo_outline`
8. `search_text`

## 9.2 Ranking Pipeline

Candidate generation:
- name/signature/summary/doc/keyword matches
- optional semantic relation matches

Scoring signals:
- exact name match
- token overlap
- signature relevance
- semantic relevance (type/call relation)
- confidence boost for semantic quality
- recency freshness (optional)

Hard requirements:
- reject empty/whitespace-only query for search tools.
- deterministic ordering when scores tie.
- explicit truncation metadata.

## 9.3 Confidence and Quality Output

Each result returns:

- `score`
- `quality_level`
- `confidence_score`
- `source_adapter`

This allows agents to decide whether to trust or request broader context.

## 10. API Contracts

## 10.1 MCP Tool Behavior

All tools return:

- `success` or `error`
- payload object
- `_meta` envelope:
    - `timing_ms`
    - `truncated`
    - `quality_stats` (semantic/syntax mix)
    - `index_version`

Error contract:
- structured error code + message + retryability flag.

## 10.2 Optional HTTP/gRPC API

Endpoints mirror MCP tools for non-agent consumers.

Recommended:
- gRPC for internal adapter/engine communication.
- HTTP JSON for external integration.

## 11. Security Model

## 11.1 Threat Model

Treat source code as sensitive customer data.  
Treat all indexed code as untrusted input.

## 11.2 Controls

- strict path and symlink validation
- parser timeouts and memory limits
- sandbox adapter execution (especially semantic adapters)
- no repository code execution
- per-tenant encryption keys (hosted)
- tamper-evident audit logs

## 11.3 Multi-Tenant Isolation (Hosted)

- tenant-scoped DB partitions/logical isolation
- tenant-specific object storage prefixes + IAM controls
- request-scoped auth context enforced in every query path
- quota/rate limiting per tenant and API key

## 12. Privacy and Compliance

## 12.1 Data Modes

- `local_only`
- `hybrid_metadata`
- `hosted_full`

## 12.2 Telemetry Policy

- Default for enterprise builds: telemetry off unless opt-in.
- Never send source code content in product telemetry.
- Separate product analytics from operational security logs.

## 12.3 Retention and Deletion

- configurable retention windows
- hard delete pipeline
- customer-request export/delete endpoints

## 13. Observability and Operations

## 13.1 Metrics

- indexing throughput (files/sec)
- parse failure rate by adapter/language
- query p50/p95/p99 latency
- index freshness lag
- semantic coverage ratio
- cache hit ratio

## 13.2 Logging

- structured JSON logs
- correlation IDs per request/job
- redaction rules for sensitive fields

## 13.3 Tracing

- OpenTelemetry spans for discovery, parse, persist, rank, serve.

## 13.4 SLO Targets (Initial)

- Query p95 < 300ms for warmed index.
- `get_symbol` p95 < 120ms.
- Incremental index update visible < 10s on small repos.

## 14. Performance and Scalability

## 14.1 Rust-Specific Strategy

- async I/O for network + storage.
- bounded thread pools for CPU-heavy parsing.
- lock minimization in hot paths.
- zero-copy/borrowed data where practical.

## 14.2 Index Storage Strategy

- content-addressed blobs reduce duplication.
- compressed symbol payloads for large repos.
- prepared statements and indexed columns for query-critical fields.

## 14.3 Scale Patterns (Hosted)

- queue-based indexing workers
- backpressure and per-tenant concurrency caps
- sharded workers by tenant/repo size

## 15. Quality Evaluation Framework

## 15.1 Evaluation Categories

1. Symbol extraction accuracy
2. Retrieval relevance
3. Latency
4. Token/cost efficiency
5. Stability under malformed code

## 15.2 Benchmark Corpus

- curated multi-language repos (open source)
- Android-heavy (Kotlin/Java)
- PHP framework-heavy (Laravel/Symfony)
- monorepos with mixed languages

## 15.3 Key Quality KPIs

- top-1/top-5 retrieval accuracy
- semantic-vs-syntax win rate
- parse error percent by language
- index drift incidents

## 16. Testing Strategy

## 16.1 Automated Test Layers

1. Unit tests
- parser utilities, scoring, schema validation

2. Adapter contract tests
- each adapter must pass common fixture suite

3. Integration tests
- end-to-end indexing + query flows

4. Security tests
- traversal, symlink escape, malformed files, resource exhaustion

5. Performance regression tests
- benchmark thresholds in CI

## 16.2 Determinism Requirements

- stable IDs across re-index when symbol identity unchanged.
- stable ordering for identical scores.
- reproducible outputs across runs for same inputs.

## 17. Monetization-Ready Platform Design

## 17.1 Commercial Packaging

Free tier:
- local-only single-user usage.

Paid hosted team tier:
- organization workspaces
- shared indexes and team access control
- SSO/SAML
- audit logs
- usage analytics
- SLA and support

Enterprise tier:
- private VPC/on-prem deployment
- advanced policy controls
- custom retention and compliance features

## 17.2 Billing Dimensions

- indexed LOC or files
- storage volume
- query volume
- concurrent indexing jobs

## 18. Migration and Compatibility

## 18.1 Versioning

- semantic versioning for APIs and index schema.
- explicit index schema version + migration tooling.

## 18.2 Backward Compatibility

- maintain N-1 API compatibility for MCP/HTTP contracts.
- provide index reindex/migrate command.

## 19. Implementation Constraints and Decisions Checklist

Before coding starts, confirm:

1. Final project name and package namespace.
2. Initial supported languages and adapter policy per language.
3. Deployment mode for first release (local-only vs hybrid vs hosted).
4. Metadata store choice (SQLite first, Postgres hosted).
5. Search engine choice (built-in index vs Tantivy).
6. Auth model (none/local, API key, OAuth/SSO).
7. Telemetry default (recommended: opt-in).
8. SLA/SLO expectations for first production target.

## 20. Definition of Done for Initial Product (V1)

V1 is done when:

- Rust MCP server supports core query/index tools.
- Tree-sitter syntax adapters run for baseline language set.
- At least two semantic adapters are integrated (implemented: TypeScript + Kotlin).
- Incremental indexing is reliable and tested.
- Security controls are implemented and validated.
- Observability dashboards show key metrics.
- Documentation covers deployment modes and data handling.
- End-to-end benchmark suite passes published quality/latency targets.

## 21. Immediate Next Step

Use this document to produce an implementation plan with:

- milestone breakdown
- crate-by-crate task graph
- staffing/ownership map
- detailed acceptance criteria per milestone

No additional product-definition documents are required before planning.

# GitHub Issue Backlog

This backlog is designed for one-PR-per-issue execution.

## Epic 0: Repository Governance and CI Foundation

- Ticket: Initialize Rust workspace and baseline repo files
  - Deliverables: `Cargo.toml` workspace root, `.gitignore`, `README.md`, `rust-toolchain.toml`.
  - DoD: workspace builds with a placeholder crate.
- Ticket: Add contribution governance files
  - Deliverables: `CONTRIBUTING.md`, `CODEOWNERS`, issue templates, PR template.
  - DoD: templates visible in GitHub UI and validated.
- Ticket: Add CI workflow for PR and push to master
  - Deliverables: workflow with fmt, clippy, tests, build, doc checks.
  - DoD: workflow required and passing.
- Manual: Configure branch protection and required checks
  - Deliverables: protected `master`, required review/checks.
  - DoD: direct pushes blocked.

## Epic 1: Workspace and Core Model

- Ticket: Create `core-model` crate with canonical schemas
- Ticket: Implement stable symbol ID construction and validation
- Ticket: Add serialization/deserialization compatibility tests
- Ticket: Add schema versioning baseline and migration contract

## Epic 2: Ingestion and Discovery

- Ticket: Implement repository walker with ignore rules
- Ticket: Add security filters (symlink/traversal/binary/size caps)
- Ticket: Add language detection with deterministic fallback
- Ticket: Add discovery metrics and structured logging

## Epic 3: Adapter API and Syntax Baseline

- Ticket: Create `adapter-api` traits and capability model
- Ticket: Create `adapter-syntax-treesitter` baseline crate
- Ticket: Implement adapter routing policy (`semantic_required/preferred/syntax_only`)
- Ticket: Add adapter contract test harness with fixtures

## Epic 4: Store and Index Commit Path

- Ticket: Create metadata store schema (SQLite-first)
- Ticket: Create content-addressed blob storage component
- Ticket: Implement staging-to-swap atomic index commit
- Ticket: Add repository/file/symbol aggregate updates

## Epic 5: Query Engine

- Ticket: Implement `search_symbols` with deterministic ranking/ties
- Ticket: Implement `get_symbol` and `get_symbols`
- Ticket: Implement file and repository outline endpoints
- Ticket: Implement `search_text` fallback

## Epic 6: MCP Server

- Ticket: Create `server-mcp` crate and tool registration
- Ticket: Implement request/response envelope with `_meta`
- Ticket: Implement structured error model (code/message/retryable)
- Ticket: Add end-to-end MCP integration tests

## Epic 7: Incremental Indexing and Reliability

- Ticket: File hash map and changed-file detection
- Ticket: Incremental reindex and deleted-file cleanup
- Ticket: Optional git-diff accelerated mode
- Ticket: Determinism and idempotency regression tests

## Epic 8: Security, Observability, Performance

- Ticket: Add OpenTelemetry spans for indexing/query pipeline
- Ticket: Add structured logs and redaction policy
- Ticket: Add security tests for malicious inputs and limits
- Ticket: Add performance benchmark job and threshold checks

## Epic 9: Semantic Adapters

- Ticket: Implement semantic adapter 1 (TypeScript recommended)
- Ticket: Implement semantic adapter 2 (PHP or Kotlin recommended)
- Ticket: Merge confidence-aware results across syntax+semantic outputs
- Ticket: Add semantic coverage metrics

## Epic 10: V1 Readiness

- Ticket: Publish architecture and operations docs
- Ticket: Add benchmark corpus and quality KPI report pipeline
- Ticket: Add compatibility policy docs (N-1 API, schema migration)
- Manual: Release readiness checklist and go/no-go review

## Manual Setup Issues to Create Immediately

- Configure repository labels.
- Configure branch protection for `master`.
- Configure required status checks once CI workflow exists.
- Configure CODEOWNERS review requirements.

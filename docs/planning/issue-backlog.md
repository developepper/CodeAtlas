# GitHub Issue Backlog

This backlog is designed for one-PR-per-issue execution.

Progress (updated 2026-03-15): Milestones M0-M11 and Epics 13 and 16 are
complete. The platform now includes indexing pipeline, query engine, MCP serving
contracts, local CLI commands, incremental indexing with git-diff acceleration
and determinism regression coverage, plus tracing, redacted structured logging,
security regression coverage, performance threshold enforcement, TypeScript and
Kotlin semantic adapters, confidence-aware merge, semantic regression gating,
semantic coverage/win-rate metrics, a built-in newline-delimited stdio MCP
server, documented client setup, packaging guidance, diagnostics coverage, tool
schemas, compatibility notes, persistent multi-repo local service, and
file-level indexing baseline for recognized languages. Workspace quality gates
are green (`fmt`, `clippy -D warnings`, full workspace tests).

## Epic 0: Repository Governance and CI Foundation (complete)

- ~~Ticket: Initialize Rust workspace and baseline repo files~~ (#12)
- ~~Ticket: Add contribution governance files~~ (#13)
- ~~Ticket: Add CI workflow for PR and push to master~~ (#14)
- ~~Manual: Configure branch protection and required checks~~ (#57, #58, #59, #56)

## Epic 1: Workspace and Core Model (complete)

- ~~Ticket: Create `core-model` crate with canonical schemas~~ (#16)
- ~~Ticket: Implement stable symbol ID construction and validation~~ (#17)
- ~~Ticket: Add serialization/deserialization compatibility tests~~ (#18)
- ~~Ticket: Add schema versioning baseline and migration contract~~ (#19)

## Epic 2: Ingestion and Discovery (complete)

- ~~Ticket: Implement repository walker with ignore rules~~ (#20)
- ~~Ticket: Add security filters (symlink/traversal/binary/size caps)~~ (#21)
- ~~Ticket: Add language detection with deterministic fallback~~ (#22)
- ~~Ticket: Add discovery metrics and structured logging~~ (#23)
- ~~Ticket: Wire indexer pipeline end-to-end~~ (#68)
- ~~Ticket: Implement enrichment stage (file summary, keywords, searchable fields)~~ (#69)

## Epic 3: Adapter API and Syntax Baseline (complete)

- ~~Ticket: Create `adapter-api` traits and capability model~~ (#24)
- ~~Ticket: Create `adapter-syntax-treesitter` baseline crate~~ (#25)
- ~~Ticket: Implement adapter routing policy~~ (#26)
- ~~Ticket: Add adapter contract test harness with fixtures~~ (#27)
- ~~Ticket: Create indexer crate and pipeline orchestration skeleton~~ (#60)

## Epic 4: Store and Index Commit Path (complete)

- ~~Ticket: Create metadata store schema (SQLite-first)~~ (#28)
- ~~Ticket: Create content-addressed blob storage component~~ (#29)
- ~~Ticket: Implement staging-to-swap atomic index commit~~ (#30)
- ~~Ticket: Add repository/file/symbol aggregate updates~~ (#31)

## Epic 5: Query Engine (complete)

- ~~Ticket: Create query-engine crate scaffold and public query trait~~ (#61, p0)
- ~~Ticket: Implement `search_symbols` with deterministic ranking/ties~~ (#32, p0)
- ~~Ticket: Implement `get_symbol` and `get_symbols`~~ (#33, p0)
- ~~Ticket: Implement file and repository outline queries~~ (#34, p1)
- ~~Ticket: Implement `search_text` fallback~~ (#35, p1)

## Epic 6: MCP Server (complete)

- ~~Ticket: Create `server-mcp` crate and tool registration~~ (#36, p0)
- ~~Ticket: Implement request/response envelope with `_meta`~~ (#37, p1)
- ~~Ticket: Implement structured error model (code/message/retryable)~~ (#38, p1)
- ~~Ticket: Add end-to-end MCP integration tests~~ (#39, p1)
- ~~Ticket: Create CLI crate for local indexing and query commands~~ (#62, p1)
- ~~Ticket: Add CLI outline commands (`file-outline`, `file-tree`, `repo-outline`)~~ (#70, p2)

## Epic 7: Incremental Indexing and Reliability (complete)

- ~~Ticket: File hash map and changed-file detection~~ (#40)
- ~~Ticket: Incremental reindex and deleted-file cleanup~~ (#41)
- ~~Ticket: Optional git-diff accelerated mode~~ (#42)
- ~~Ticket: Determinism and idempotency regression tests~~ (#43)

## Epic 8: Security, Observability, Performance (complete)

- ~~Ticket: Add OpenTelemetry spans for indexing/query pipeline~~ (#44)
- ~~Ticket: Add structured logs and redaction policy~~ (#45)
- ~~Ticket: Add security tests for malicious inputs and limits~~ (#46)
- ~~Ticket: Add performance benchmark job and threshold checks~~ (#47)

## Epic 9: Semantic Adapters (complete)

- ~~Ticket: Implement semantic adapter 1 (TypeScript recommended)~~ (#48)
- ~~Ticket: Implement semantic adapter 2 (PHP or Kotlin recommended)~~ (#49)
- ~~Ticket: Merge confidence-aware results across syntax+semantic outputs~~ (#50)
- ~~Ticket: Add semantic coverage metrics~~ (#51)
- ~~Ticket: TypeScript semantic runtime integration and lifecycle~~ (#64)
- ~~Ticket: TypeScript semantic symbol mapping baseline~~ (#65)
- ~~Ticket: Kotlin semantic runtime integration and symbol extraction baseline~~ (#66)
- ~~Ticket: Semantic adapter quality regression suite and gating criteria~~ (#67)

## Epic 10: V1 Readiness (complete)

- ~~Ticket: Publish architecture and operations docs~~
- ~~Ticket: Add benchmark corpus and quality KPI report pipeline~~
- ~~Ticket: Add compatibility policy docs (N-1 API, schema migration)~~
- ~~Manual: Release readiness checklist and go/no-go review~~

## Epic 11: First-Class Local MCP Server for AI Clients (complete)

- ~~Ticket: Add codeatlas mcp serve canonical CLI entrypoint and server wiring~~ (#135)
- ~~Ticket: Implement stdio JSON-RPC framing and MCP request routing~~ (#131)
- ~~Ticket: Add MCP tool schemas for all existing CodeAtlas tools~~ (#133)
- ~~Ticket: Add MCP diagnostics and subprocess integration coverage~~ (#132)
- ~~Ticket: Publish supported MCP client setup and troubleshooting docs~~ (#134)
- ~~Ticket: Add MCP packaging and installation path for end users~~ (#136)
- ~~Ticket: Validate MCP client compatibility and add minimal interoperability shims~~ (#137)

## Post-V1 Direction

Strategic roadmap themes after M10 live in:

- `docs/planning/post-v1-roadmap.md`
- `docs/planning/persistent-multi-repo-local-service.md`
- `docs/planning/universal-syntax-indexing.md`

### Epic 13: Persistent Multi-Repo Local Service (complete)

The first post-v1 execution slice is complete. Planning artifact:
`docs/planning/persistent-multi-repo-local-service.md`.

GitHub epic and tickets (all complete):

- ~~#148 Epic 13: Persistent Multi-Repo Local Service~~
- ~~#149 Ticket: Define the persistent multi-repo local service architecture~~
- ~~#150 Ticket: Make shared-store usage canonical and add missing repo catalog metadata~~
- ~~#151 Ticket: Add repo catalog and lifecycle operations for a persistent local service~~
- ~~#152 Ticket: Implement a persistent local service runtime~~
- ~~#153 Ticket: Adapt AI client integration for the persistent service model~~
- ~~#154 Ticket: Update docs and canonical usage guidance for the persistent local model~~

What was delivered:

- persistent HTTP service (`codeatlas serve`)
- repo catalog with lifecycle operations (`codeatlas repo add/list/status/refresh/remove`)
- MCP bridge for AI client integration (`codeatlas mcp bridge`)
- shared storage root and repo catalog metadata
- canonical doc updates

Deferred from the first slice:

- Docker packaging
- broader cross-repo search/dependency features
- remote connectors
- hosted/team controls

### Epic 16: Non-Empty Index Baseline For Recognized Files (complete)

Planning artifact:
`docs/planning/recognized-language-file-indexing.md`.

GitHub epic and tickets (all complete):

- ~~#164 Epic 16: Non-Empty Index Baseline For Recognized Files~~
- ~~#166 Ticket: Persist file records and blobs for recognized files without symbol adapters~~
- ~~#167 Ticket: Wire file-content retrieval and file-level query behavior to the fallback model~~
- ~~#165 Ticket: Update metrics, docs, and benchmark guidance for file-level indexing coverage~~

What was delivered:

- recognized files persist as file-level index entries even without symbols
- blob-backed file content retrieval wired to production query/service path
- file tree, file outline, and repo outline work on file-only indexed repos
- missing-adapter and adapter-failure paths are diagnostically distinct
- `files_file_only` metric tracks file-level indexed files separately from `files_parsed`
- quality report returns `NOT APPLICABLE` for repos with zero symbol coverage
- docs, runbook, benchmark guidance, and CLI output updated for file-level baseline

### Epic 17: Universal Syntax Indexing Platform (next)

Planning artifact:
`docs/planning/universal-syntax-indexing.md`.

Canonical technical design source:
`docs/architecture/universal-syntax-indexing-architecture.md`.

Reviewed issue docs for the next issue-creation pass:

- `docs/planning/github-issues/universal-syntax-indexing-epic.md`
- `docs/planning/github-issues/universal-syntax-ticket-architecture.md`
- `docs/planning/github-issues/universal-syntax-ticket-core-model-metrics.md`
- `docs/planning/github-issues/universal-syntax-ticket-syntax-platform.md`
- `docs/planning/github-issues/universal-syntax-ticket-php.md`
- `docs/planning/github-issues/universal-syntax-ticket-python.md`
- `docs/planning/github-issues/universal-syntax-ticket-go.md`
- `docs/planning/github-issues/universal-syntax-ticket-java.md`
- `docs/planning/github-issues/universal-syntax-ticket-javascript.md`
- `docs/planning/github-issues/universal-syntax-ticket-query-surfaces.md`
- `docs/planning/github-issues/universal-syntax-ticket-benchmarks-docs.md`

Intent:

- make syntax indexing the default baseline for major recognized code languages
- treat semantic indexing as enrichment layered on top of syntax
- keep file-only indexing as the explicit last fallback rather than the common
  case
- refactor schema, routing, crate boundaries, and query expectations where
  needed for the long-term architecture

Candidate child tickets:

- define the universal syntax indexing architecture and capability model
- refactor core model and metrics for file/syntax/semantic capability tiers
- create the multi-language syntax indexing subsystem and migrate Rust onto it
- implement PHP syntax indexing as the proving ground
- implement Python syntax indexing
- implement Go syntax indexing
- implement Java syntax indexing
- implement JavaScript syntax indexing
- rework query surfaces for broad syntax coverage
- update benchmark and token-efficiency evaluation methodology

Planning stance:

- correctness and long-term architecture are favored over backward
  compatibility for this initiative
- this epic is intentionally allowed to perform foundational refactors rather
  than preserving awkward intermediate interfaces

## Manual Setup Issues to Create Immediately

- Configure repository labels.
- Configure branch protection for `master`.
- Configure required status checks once CI workflow exists.
- Configure CODEOWNERS review requirements.

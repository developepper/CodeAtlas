# Rust Code Intelligence Implementation Plan

Source: `docs/specifications/rust-code-intelligence-platform-spec.md`

## Project Setup Decisions

- Repository default branch: `master`.
- CI platform: GitHub Actions.
- Initial deployment target: local-only first, with hosted-ready architecture boundaries.
- Quality policy: fail-fast CI (fmt, clippy, tests, build, docs checks).

## Rust Toolchain and MSRV Policy (Best Practice)

- Use `rust-toolchain.toml` pinned to `stable` for contributor consistency.
- Set explicit `rust-version` in `Cargo.toml` once crates exist.
- Add CI jobs for:
  - pinned MSRV (`cargo check`)
  - stable (`fmt`, `clippy`, `test`, `doc`)
- MSRV bump requires dedicated ticket and changelog entry.

## Crate Architecture (current)

```
crates/
  core-model/               Canonical schemas (SymbolRecord, FileRecord, RepoRecord, SymbolKind)
  repo-walker/              Repository traversal, ignore rules, language detection, security filters
  syntax-platform/          Tree-sitter grammar registry, SyntaxBackend trait, language modules
  semantic-api/             SemanticBackend trait and shared semantic types
  adapter-semantic-typescript/ TypeScript semantic extraction via tsserver (package: semantic-typescript)
  adapter-semantic-kotlin/  Kotlin semantic extraction via JVM analysis bridge (package: semantic-kotlin)
  store/                    MetadataStore (SQLite), BlobStore (content-addressed filesystem)
  indexer/                  Pipeline orchestration: discovery → extract → enrich → persist
  query-engine/             Ranked symbol/text retrieval and structure queries
  server-mcp/               MCP tool registry and response/error contracts
  cli/                      Local command surface for indexing and query workflows
  workspace-placeholder/    Cargo workspace anchor
```

### Indexer pipeline stages

1. **Discovery** — walks repo, detects languages, loads file content
2. **Parse** — routes files to adapters, extracts symbols with fallback
3. **Enrich** — heuristic file summaries, symbol summaries, keyword extraction
4. **Persist** — blob writes, metadata transaction (stale cleanup, upserts, aggregate recompute)

Key design decisions:
- PipelineContext is immutable; stores passed separately to persist stage
- Stale cleanup uses discovery output (not parse output) for failure isolation
- Repo upsert uses INSERT OR IGNORE + UPDATE to avoid ON DELETE CASCADE
- Aggregates (file_count, symbol_count, language_counts) recomputed from DB state
- Per-file symbol cleanup before upsert handles renamed/removed symbols
- Incremental indexing uses persisted file hashes, deleted-file cleanup, and
  optional git-diff acceleration via persisted `git_head` plus dirty working-tree detection
- Determinism is guarded by regression coverage for stable IDs, ordering, and
  incremental vs fresh-index equivalence
- Milestone 8 adds tracing spans across indexing/query/MCP flows, structured
  CLI logging with redaction, security regression suites, and CI-enforced
  performance thresholds plus Criterion benchmarks
- Milestone 9 adds TypeScript and Kotlin semantic adapters, confidence-aware
  syntax+semantic merge, regression KPI gating, and semantic coverage/win-rate
  reporting

## Milestone/Epic Breakdown

1. ~~Epic 0: Repository Governance and CI Foundation~~ (complete)
2. ~~Epic 1: Workspace/Crate Skeleton and Core Model~~ (complete)
3. ~~Epic 2: Ingestion and Discovery Pipeline~~ (complete)
4. ~~Epic 3: Adapter API and Tree-sitter Syntax Baseline~~ (complete)
5. ~~Epic 4: Storage and Atomic Index Commit Path~~ (complete)
6. ~~Epic 5: Query Engine and Deterministic Ranking~~ (complete)
7. ~~Epic 6: MCP Server Interface and Tool Contracts~~ (complete)
8. ~~Epic 7: Incremental Indexing and Reliability Hardening~~ (complete)
9. ~~Epic 8: Security, Observability, and Performance Guardrails~~ (complete)
10. ~~Epic 9: Semantic Adapter Integration (at least two languages)~~ (complete)
11. Epic 10: Documentation, Benchmarks, and V1 Readiness Review

Post-V1 direction:
- `docs/planning/post-v1-roadmap.md`

## Non-Code Manual Work

- ~~Configure branch protection on `master`.~~ (done)
- ~~Configure required checks and CODEOWNERS review.~~ (done)
- ~~Configure repository labels and issue forms.~~ (done)
- Configure secrets/settings for future hosted integrations.

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
  adapter-api/              LanguageAdapter trait, AdapterRouter, capability model, contract tests
  adapter-syntax-treesitter/ Tree-sitter based syntax extraction (Rust support)
  store/                    MetadataStore (SQLite), BlobStore (content-addressed filesystem)
  indexer/                  Pipeline orchestration: discovery → parse → enrich → persist
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

## Milestone/Epic Breakdown

1. ~~Epic 0: Repository Governance and CI Foundation~~ (complete)
2. ~~Epic 1: Workspace/Crate Skeleton and Core Model~~ (complete)
3. ~~Epic 2: Ingestion and Discovery Pipeline~~ (complete)
4. ~~Epic 3: Adapter API and Tree-sitter Syntax Baseline~~ (complete)
5. ~~Epic 4: Storage and Atomic Index Commit Path~~ (complete)
6. **Epic 5: Query Engine and Deterministic Ranking** (next)
7. Epic 6: MCP Server Interface and Tool Contracts
8. Epic 7: Incremental Indexing and Reliability Hardening
9. Epic 8: Security, Observability, and Performance Guardrails
10. Epic 9: Semantic Adapter Integration (at least two languages)
11. Epic 10: Documentation, Benchmarks, and V1 Readiness Review

## Non-Code Manual Work

- ~~Configure branch protection on `master`.~~ (done)
- ~~Configure required checks and CODEOWNERS review.~~ (done)
- ~~Configure repository labels and issue forms.~~ (done)
- Configure secrets/settings for future hosted integrations.

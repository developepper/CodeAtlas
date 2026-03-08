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

## Milestone/Epic Breakdown

1. Epic 0: Repository Governance and CI Foundation
2. Epic 1: Workspace/Crate Skeleton and Core Model
3. Epic 2: Ingestion and Discovery Pipeline
4. Epic 3: Adapter API and Tree-sitter Syntax Baseline
5. Epic 4: Storage and Atomic Index Commit Path
6. Epic 5: Query Engine and Deterministic Ranking
7. Epic 6: MCP Server Interface and Tool Contracts
8. Epic 7: Incremental Indexing and Reliability Hardening
9. Epic 8: Security, Observability, and Performance Guardrails
10. Epic 9: Semantic Adapter Integration (at least two languages)
11. Epic 10: Documentation, Benchmarks, and V1 Readiness Review

## Non-Code Manual Work

- Configure branch protection on `master`.
- Configure required checks and CODEOWNERS review.
- Configure repository labels and issue forms.
- Configure secrets/settings for future hosted integrations.

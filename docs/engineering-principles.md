# Engineering Principles

## Quality Bar

- Every issue maps to exactly one PR.
- Every PR has a single concern and a measurable acceptance target.
- All behavior changes require tests or a documented rationale for why tests are not applicable.
- No TODO/FIXME may be merged without a linked issue.
- Public interfaces must include docs and error semantics.

## Rust Standards

- Enforce formatting via `cargo fmt --check`.
- Enforce linting via `cargo clippy --all-targets --all-features -- -D warnings`.
- Prefer explicit error types and `thiserror`/`anyhow` layering by boundary.
- Prefer deterministic outputs (stable ordering, stable IDs, reproducible test fixtures).
- Avoid `unsafe` unless reviewed and justified with a dedicated safety comment and test coverage.

## Testing Policy

- Unit tests for pure logic.
- Contract tests for adapter behavior.
- Integration tests for indexing/query workflows.
- Security tests for path traversal, symlink escape, malformed files, and resource exhaustion.
- Performance regression tests for agreed p95/p99 targets before release milestones.

## Definition Of Done (Per Ticket)

- Acceptance criteria implemented.
- Tests added/updated and passing.
- Docs updated (if user-facing behavior changed).
- CI green.
- PR links exactly one primary issue.

## Review Checklist

- Scope limited to issue intent.
- Correctness, edge cases, and failure handling validated.
- Security and data handling implications reviewed.
- Observability impact reviewed (logs/metrics/traces where relevant).
- Migration/compatibility impact reviewed (schema/API/index versioning).

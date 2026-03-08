# Contributing

## Workflow

1. Create or pick one ticket issue.
2. Create a branch named `ticket/<issue-id>-<slug>`.
3. Implement only the scope in that issue.
4. Run local checks:
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all --all-features`
5. Open PR using the template and link `Closes #<issue-id>`.

## Standards

- One PR per issue.
- Single concern per PR.
- Tests required for behavior changes.
- Deterministic behavior required for ranking/IDs/output ordering.
- Security-sensitive changes require threat/risk notes in PR description.

## Commit Guidance

- Keep commits focused and reviewable.
- Prefer conventional style: `feat:`, `fix:`, `chore:`, `docs:`, `test:`.

## Review Expectations

- CI must pass.
- CODEOWNERS approval required where configured.
- Do not merge with unresolved review comments.

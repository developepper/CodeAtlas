# GitHub Workflow

## Branching

- Default branch: `master`.
- Branch naming:
  - `epic/<issue-id>-<slug>` only when explicitly approved for umbrella docs/meta work.
  - `ticket/<issue-id>-<slug>` for normal implementation PRs.

## One Issue, One PR Rule

- Every ticket issue must be resolved in one PR.
- PR must reference one primary issue in the description (`Closes #<id>`).
- If scope grows beyond ticket acceptance criteria, open a follow-up issue instead of expanding the PR.

## Issue Types

- `epic`: outcome-level tracking.
- `ticket`: single PR deliverable.
- `manual`: non-code/ops intervention.

## Pull Request Requirements

- Use PR template.
- Include test evidence and any benchmark/security evidence required by the issue.
- Keep PR reviewable; target under ~500 changed lines excluding fixtures and generated files.
- Require passing CI before merge.

## Merge Policy

- Squash merge by default for clean history.
- Require at least one approving review.
- Require all required status checks.
- No direct pushes to `master`.

## Labels

- Core labels: `epic`, `ticket`, `manual`, `ci`, `docs`, `rust`, `testing`, `security`, `blocked`.
- Priority labels: `p0`, `p1`, `p2`.

## Manual Guardrails

- Branch protection for `master` must enforce:
  - PR required
  - status checks required
  - review required
  - dismissal of stale reviews on new commits

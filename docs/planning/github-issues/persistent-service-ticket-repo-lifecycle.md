## Problem

Users need service-level operations for adding, listing, refreshing, and
removing repos. Without a repo catalog UX, a shared service becomes a hidden
database rather than a usable multi-project tool.

## Scope

- add repo registration/indexing commands or APIs
- add repo listing and discovery operations
- add refresh/reindex operations per repo
- add removal/de-registration operations
- expose repo status, source root, last indexed time, and freshness basics

## Deliverables

- repo lifecycle command/API surface
- repo listing/status output
- documentation for repo registration and switching workflows

## Acceptance Criteria

- [ ] a user can add multiple repos to one CodeAtlas instance
- [ ] a user can list all indexed or known repos
- [ ] a user can refresh or remove a repo without affecting others
- [ ] lifecycle operations surface registration state, index state, and freshness clearly
- [ ] repo discovery is clear enough that AI tooling or wrappers can choose the correct `repo_id`

## Testing Requirements

- Integration: add/list/refresh/remove flows
- Negative: duplicate, replaced, or conflicting repo identity cases
- Regression: repo removal or refresh does not affect unrelated repos

## Dependencies

- Parent epic: #148
- Depends on #150

## Review Checklist

- lifecycle operations are complete enough for real daily use
- repo identity collisions are handled intentionally
- status output is understandable

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)
- GitHub issue: #151

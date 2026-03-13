## Problem

Once the persistent multi-repo service exists, the docs must stop presenting
per-repo and per-process usage as the dominant mental model and must explain
the CLI migration path clearly.

## Scope

- update README usage guidance
- update deployment docs
- update operations runbook
- update roadmap and backlog references as needed
- document the direct-store versus service-first CLI distinction during the
  transition
- keep docs explicit about what is implemented versus future hosted direction

## Deliverables

- updated user-facing docs
- updated operator-facing docs
- review notes summarizing the new canonical flow

## Acceptance Criteria

- [ ] docs present the persistent local service as the canonical local model
- [ ] docs explain how multi-repo usage works
- [ ] docs explain the CLI migration story clearly enough to avoid user confusion
- [ ] docs explain the relationship between local self-hosted and future managed deployment
- [ ] docs remain honest about unsupported hosted features

## Testing Requirements

- Unit: not required
- Integration: not required
- Review: manual doc review for accuracy against implementation

## Dependencies

- Parent epic: #148
- Depends on #149
- Depends on #150
- Depends on #151
- Depends on #152
- Depends on #153

## Review Checklist

- user journey is easy to follow
- no stale per-repo assumptions remain in primary docs
- local versus future hosted boundaries are still clear

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [README.md](README.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- GitHub issue: #154

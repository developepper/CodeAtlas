## Problem

A persistent multi-repo product needs a real long-running service process with
clear startup, storage-root ownership, lifecycle handling, and diagnostics.

## Scope

- add a long-running local service mode
- define canonical startup and stop behavior
- configure service storage root and runtime state
- add HTTP health and status behavior appropriate to the chosen transport
- ensure logs and diagnostics are operationally usable

## Deliverables

- runnable persistent local service entrypoint
- startup and configuration docs
- health and status behavior

## Acceptance Criteria

- [ ] one CodeAtlas instance can stay running across multiple repo workflows
- [ ] startup configuration is explicit and documented
- [ ] service health can be checked without digging through internals
- [ ] diagnostics are clear on startup and runtime failure

## Testing Requirements

- Integration: service startup and shutdown
- Runtime: shared-store operation over multiple repos
- Diagnostics: invalid storage or config cases
- Performance: avoid obvious service startup overhead regressions

## Dependencies

- Parent epic: Epic 13 persistent local service issue
- Depends on the architecture-definition ticket
- Depends on the shared-store/catalog ticket

## Notes

- This ticket can proceed in parallel with the repo lifecycle ticket once the
  architecture and shared-store assumptions are settled.

## Review Checklist

- service runtime is stable
- storage root ownership is clear
- diagnostics are actionable
- async runtime concerns are contained to the service path where practical

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)

## Problem

The repository documents local-first and hosted-ready boundaries, but it does
not yet define the canonical architecture for a persistent multi-repo local
service or how that service should relate to MCP and future centralized
deployment.

## Scope

- define the canonical local service model
- define how it differs from the current per-repo/per-process model
- confirm or adjust the recommended HTTP-plus-MCP-bridge direction
- define how MCP compatibility fits into the new model
- define the CLI migration strategy from direct `--db` usage to service-first
  usage
- define the architectural relationship between local self-hosted and future
  centralized deployments
- document the explicit refactor-first policy for this initiative

## Deliverables

- architecture doc for the persistent local service
- updated deployment-mode guidance reflecting the new canonical local model
- explicit decision record for transport and service boundaries
- explicit decision record for CLI migration boundaries

## Acceptance Criteria

- [ ] the canonical local product shape is documented unambiguously
- [ ] the doc explains how multi-repo local and future centralized modes share one architectural direction
- [ ] the transport strategy is explicit enough to unblock implementation tickets
- [ ] the CLI migration strategy is explicit enough to unblock service and client integration tickets
- [ ] the docs state that correctness/refactor quality is favored over backward compatibility for this early-stage initiative

## Testing Requirements

- Unit: not required
- Integration: not required
- Security: not required
- Review: confirm the document is actionable enough to start implementation
  without hidden assumptions

## Dependencies

- Parent epic: Epic 13 persistent local service issue

## Review Checklist

- service boundaries are clear
- transport decision is explicit
- hosted relationship is clear
- CLI migration path is explicit
- no accidental commitment to unsupported features

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)

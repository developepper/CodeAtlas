## Objective

Make CodeAtlas usable as one persistent local code-intelligence backend across
many repositories and AI clients, while preserving an architecture that can be
extended into a managed centralized service later.

## Problem

The current per-repo/per-process model creates daily friction for developers
who work across multiple repositories and want one stable AI integration point.
It also makes Docker and future hosted deployment awkward because the primary
integration path is currently stdio MCP tied to a single spawned process.

## In Scope

- define the persistent multi-repo local service product model
- make shared-store usage the canonical local shape
- add repo catalog and lifecycle operations for many repos in one service
- add a long-running local service runtime
- add a service-compatible AI integration path while preserving MCP
  compatibility
- document the relationship between self-hosted local and future managed
  deployment

## Out Of Scope

- full hosted/team feature implementation
- auth, RBAC, quotas, billing, or multi-tenant controls
- organization/workspace UI
- unrelated query-surface expansion
- Docker packaging in the first implementation slice

## Child Tickets

- Ticket: Define the persistent multi-repo local service architecture
- Ticket: Make shared-store usage canonical and add missing repo catalog metadata
- Ticket: Add repo catalog and lifecycle operations for a persistent local service
- Ticket: Implement a persistent local service runtime
- Ticket: Adapt AI client integration for the persistent service model
- Ticket: Update docs and canonical usage guidance for the persistent local model

## Epic Definition Of Done

- a user can run one persistent CodeAtlas instance and index/query multiple
  repos through it
- repository identity and discovery are clear and stable
- AI client integration does not require one separate CodeAtlas runtime per repo
- the architecture cleanly supports both self-hosted local and future managed
  deployment modes
- docs reflect the new canonical local usage model
- Docker support is explicitly deferred or split into follow-up work unless it
  proves necessary to validate the core service model

## Review Evidence Required

- architecture/design doc updates merged
- acceptance evidence per child ticket
- end-to-end proof of multi-repo indexing and querying
- explicit note on transport choice and rationale
- explicit note on CLI migration shape and rationale

## Notes

- This issue is intended to occupy the concrete implementation slot for
  post-v1 roadmap Epic 13: Multi-Repo Intelligence.
- It intentionally absorbs the local-service portion of Epic 12 work where repo
  lifecycle, freshness, and service health are required.
- Docker packaging is a follow-up, not part of the first implementation epic.

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/planning/issue-backlog.md](docs/planning/issue-backlog.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)

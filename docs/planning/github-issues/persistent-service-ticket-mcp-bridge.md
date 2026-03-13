## Problem

The current AI integration story centers on stdio MCP launched directly by the
client. A persistent service needs a bridge model so AI clients can use the
long-running backend without spawning isolated per-repo instances.

## Scope

- implement the MCP-to-service bridge model
- preserve a practical MCP compatibility story
- ensure repo-scoped operations remain clear in multi-repo usage
- avoid leaking transport-specific complexity into core query logic
- make the intended user-facing MCP setup flow explicit

## Deliverables

- MCP bridge or launcher path for the persistent service
- updated MCP and service docs
- end-to-end example flow from AI client to the multi-repo backend
- explicit recommended client configuration shape for the first slice

## Acceptance Criteria

- [ ] AI clients can use the persistent service without one CodeAtlas runtime per repo
- [ ] the integration path is documented clearly enough for real setup
- [ ] the intended user-facing MCP setup flow is explicit, not implicit
- [ ] repo targeting and discovery are understandable in the client workflow
- [ ] the implementation preserves clean separation between transport and query semantics
- [ ] bridge behavior is explicit rather than hidden in ambiguous auto-detection

## Testing Requirements

- Integration: chosen client and service path
- Subprocess or service tests: handshake and query flows as applicable
- Regression: stdout and log discipline where MCP compatibility remains relevant

## Dependencies

- Parent epic: Epic 13 persistent local service issue
- Depends on the architecture-definition ticket
- Depends on the repo lifecycle ticket
- Depends on the persistent runtime ticket

## Review Checklist

- client setup is actually simpler than the current model
- repo selection is practical
- transport boundary stays clean
- the bridge UX does not create more operator friction than it removes

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [docs/architecture/mcp-client-compatibility.md](docs/architecture/mcp-client-compatibility.md)

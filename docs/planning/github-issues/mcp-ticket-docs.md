## Problem

The end-user goal is simple AI-client setup, but the current docs still frame
MCP as an embeddable library surface. Without concrete client setup examples,
the implementation will still be hard for users to adopt.

## Scope

- update README to present `codeatlas mcp serve --db <path>` as the supported MCP flow once implemented
- add copy-paste setup guidance for a small set of real MCP clients
- include troubleshooting for bad DB paths, startup failures, and validation steps
- align architecture and operations docs with the supported local MCP story
- avoid documenting unsupported hosted or non-stdio modes

## Acceptance Criteria

- [ ] README documents the supported MCP launch flow using `codeatlas mcp serve --db <path>`
- [ ] docs include copy-pasteable setup guidance for Claude Desktop and Cursor
- [ ] docs include one additional ChatGPT/Codex-style local MCP client or wrapper with stable config shape at implementation time
- [ ] docs include a basic troubleshooting section for startup and DB-path failures
- [ ] docs do not overstate support for hosted or non-stdio deployment modes

## Testing Requirements

- Unit: not required
- Integration: manually validate documented config examples against the implemented server where practical
- Security: ensure docs do not recommend unsafe logging or source-sharing practices
- Performance: not required

## Dependencies

- Parent epic: #130
- Depends on #135
- Depends on #131
- Depends on #133
- Depends on #132

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing where applicable
- [ ] Docs updated
- [ ] CI green if doc checks exist

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [README.md](README.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)

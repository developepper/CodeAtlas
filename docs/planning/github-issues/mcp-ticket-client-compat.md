## Problem

Different MCP clients are nominally compatible while still having small
behavioral expectations around lifecycle methods, empty capability responses,
or startup conventions. Without targeted compatibility validation and minor
shims where needed, CodeAtlas risks being "spec-correct" but frustrating in
real clients.

## Scope

- identify the minimum set of client-specific interoperability expectations for
  the first documented MCP clients
- add small compatibility shims only where they are needed and do not distort
  the core server model
- validate startup and handshake behavior against the documented clients
- document any intentional compatibility accommodations and their rationale
- avoid broad client-specific branching or unsupported feature creep
- include validation that newline-delimited JSON misconfiguration fails clearly
  rather than appearing to work

## Acceptance Criteria

- [ ] documented target clients have their startup and handshake behavior validated against the implemented server
- [ ] any required compatibility shims are small, explicit, and covered by tests or manual validation notes
- [ ] client-specific accommodations do not leak non-protocol output to stdout
- [ ] compatibility notes are documented where needed for maintainability
- [ ] the server remains generic stdio MCP infrastructure rather than becoming tightly coupled to a single client
- [ ] client-specific shims are treated as additive and only required when a documented client demonstrably needs them

## Testing Requirements

- Unit: targeted tests for any compatibility-specific behavior that can be exercised locally
- Integration: validate handshake behavior against representative client expectations and subprocess flows
- Security: ensure compatibility changes do not broaden exposed surface or weaken output discipline
- Performance: not required

## Dependencies

- Parent epic: #130
- Depends on #131
- Depends on #132
- Depends on #134

## Notes

- Compatibility validation is part of the first supported release for the
  documented clients.
- Client-specific shims should be fast-follow unless validation shows that a
  documented client requires them for basic interoperability.

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing where applicable
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [README.md](README.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)

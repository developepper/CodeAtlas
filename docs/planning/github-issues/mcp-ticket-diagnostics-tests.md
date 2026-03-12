## Problem

Even a correct protocol implementation is hard to operate if startup failures,
DB problems, or malformed requests are poorly surfaced. MCP clients also need
high-confidence subprocess coverage because stdio framing bugs are easy to miss
with unit tests alone.

## Scope

- harden startup/runtime diagnostics for missing DB, unreadable DB, and schema/open failures
- ensure all diagnostics remain off stdout
- add subprocess integration tests that cover newline-delimited stdio behavior
  end to end
- add smoke coverage for `initialize -> tools/list -> tools/call`
- assert that invalid requests are reported predictably

## Acceptance Criteria

- [ ] startup failures provide actionable stderr diagnostics for missing or unreadable DB paths
- [ ] invalid or malformed requests produce structured errors or clear failure behavior without corrupting stdout
- [ ] subprocess integration tests cover handshake, tool listing, and at least one real tool call
- [ ] tests assert that stdout contains only protocol frames
- [ ] failure-path behavior is documented in code/tests clearly enough to prevent regressions

## Testing Requirements

- Unit: targeted error-mapping tests where useful
- Integration: subprocess stdio tests for success and failure paths
- Security: verify diagnostics do not include raw source content or stdout corruption
- Performance: not required

## Dependencies

- Parent epic: #130
- Depends on #131
- Depends on #133

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [crates/cli/src/main.rs](crates/cli/src/main.rs)

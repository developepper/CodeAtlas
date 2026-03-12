## Problem

There is no supported end-user command for launching CodeAtlas as an MCP server.
Without a canonical CLI entrypoint, setup remains wrapper-dependent and harder
to document or support.

## Scope

- add an `mcp` command family to the CLI
- add `codeatlas mcp serve --db <path>` as the canonical launch path
- validate required CLI arguments and startup preconditions
- wire the command to a reusable MCP server runner
- keep stdio transport code outside the core `server-mcp` registry crate
- ensure server startup and shutdown behavior is deterministic

## Acceptance Criteria

- [ ] `codeatlas mcp serve --db <path>` is a valid CLI command
- [ ] startup fails clearly when `--db` is missing or unreadable
- [ ] the CLI path invokes shared MCP server logic rather than duplicating tool registry behavior
- [ ] `server-mcp` remains focused on reusable tool registry/business logic
- [ ] command help/usage text reflects the new `mcp` command family

## Testing Requirements

- Unit: argument parsing and validation tests for the new CLI path
- Integration: command startup behavior for valid and invalid DB paths
- Security: verify logs/diagnostics stay off stdout during startup failures
- Performance: not required beyond avoiding unnecessary startup overhead

## Dependencies

- Parent epic: #130

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [crates/cli/src/main.rs](crates/cli/src/main.rs)
- [crates/server-mcp/src/lib.rs](crates/server-mcp/src/lib.rs)

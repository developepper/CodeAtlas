## Problem

`tools/list` requires JSON Schema input definitions for each tool. The current
registry exposes tool names but not schemas, so clients cannot reliably render
or validate tool inputs.

## Scope

- define JSON Schema input metadata for all existing MCP tools
- ensure the schema set matches the parameter structs in
  `crates/server-mcp/src/tools.rs`
- return those schemas from `tools/list`
- include stable names, descriptions, required fields, and optional fields
- avoid inventing new tools or changing existing tool semantics in this ticket

## Acceptance Criteria

- [ ] every existing CodeAtlas MCP tool is included in `tools/list`
- [ ] each tool has a JSON Schema input definition matching its current parameters
- [ ] required versus optional fields are represented correctly
- [ ] tool names in `tools/list` match the names accepted by `ToolRegistry`
- [ ] schema output is stable and suitable for snapshot-style testing

## Testing Requirements

- Unit: schema generation/serialization tests per tool or grouped snapshot tests
- Integration: `tools/list` response includes all tools and schemas through the stdio server path
- Security: ensure schemas do not expose internal-only implementation details
- Performance: not required

## Dependencies

- Parent epic: #130
- Depends on #131

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)
- [crates/server-mcp/src/registry.rs](crates/server-mcp/src/registry.rs)

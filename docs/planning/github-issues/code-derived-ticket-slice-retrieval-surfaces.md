## Problem

Even if exact slice retrieval exists in the store/query layer, it will not
improve real user workflows until it is exposed through the service, MCP, and
CLI surfaces that agents actually use.

## Scope

- add service HTTP support for exact slices
- add MCP and CLI contracts for exact slices
- document the user-facing exact-slice behavior
- add integration coverage for the external surfaces

## Deliverables

- service/MCP/CLI exact-slice surfaces
- user-facing docs for exact slice retrieval
- integration coverage for the exposed contracts

## Acceptance Criteria

- [ ] service endpoints expose exact slice retrieval coherently
- [ ] MCP clients can request exact slices without falling back to whole-file retrieval
- [ ] CLI users can request exact slices directly
- [ ] docs and examples remain consistent with shipped exact-slice behavior

## Testing Requirements

- Integration: service/MCP/CLI exact-slice retrieval
- Docs validation: examples remain consistent with shipped behavior
- Regression: external slice surfaces remain deterministic across repeated calls

## Dependencies

- Requires architecture ticket
- Requires the core exact-slice retrieval ticket

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [crates/service/src/routes.rs](crates/service/src/routes.rs)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)
- [crates/cli/src/main.rs](crates/cli/src/main.rs)

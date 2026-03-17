## Problem

Even with persisted edges, CodeAtlas will still feel like a search tool rather
than a repository intelligence system unless those edges are exposed through
clean graph-aware query surfaces.

## Scope

- add query APIs for graph neighbors and related artifacts
- support callers/callees, references, dependency neighbors, and related-symbol
  style lookups where backing edges exist
- expose coverage and explainability rather than implying perfect graph depth

## Deliverables

- graph-aware query/service/MCP surfaces
- docs for query semantics and edge coverage expectations
- integration coverage for representative graph queries

## Acceptance Criteria

- [ ] callers/callees or equivalent graph-neighbor queries work where backing edges exist
- [ ] references or dependency-neighbor queries expose evidence-backed results
- [ ] results remain explicit about missing or partial edge coverage
- [ ] graph queries are stable and deterministic across repeated calls

## Testing Requirements

- Unit: query-layer branching and ranking where practical
- Integration: service/MCP/query-engine graph query coverage
- Diagnostics: missing-coverage cases are reported coherently

## Dependencies

- Requires architecture ticket
- Requires graph persistence ticket

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [crates/query-engine/src/store_service.rs](crates/query-engine/src/store_service.rs)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)

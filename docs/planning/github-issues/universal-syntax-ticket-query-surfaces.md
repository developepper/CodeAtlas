## Problem

The query layer was shaped around sparse symbol coverage. Once syntax indexing
becomes the normal baseline for many languages, query behavior and contracts
must remain clean and useful across file, syntax, and semantic capability tiers.

## Scope

- review and rework query behavior for broad syntax coverage
- ensure file outline and symbol search remain clean across capability tiers
- add or improve exact slice retrieval if needed for syntax-first workflows

## Deliverables

- query/service updates
- capability-tier behavior documentation
- integration coverage for syntax-indexed repos

## Acceptance Criteria

- [ ] file outline returns:
      - file metadata plus an empty symbol list for file-only repos
      - syntax-derived symbols for syntax-indexed repos
      - merged symbols for syntax-plus-semantic repos
- [ ] symbol search returns stable, non-empty results on representative
      syntax-indexed repositories where relevant symbols exist
- [ ] exact symbol lookup continues to work across syntax-indexed and
      syntax-plus-semantic repositories
- [ ] exact file/source retrieval semantics remain coherent across capability tiers

## Testing Requirements

- Unit: query-layer behavior tests for capability-tier branching where practical
- Integration: service/MCP/query-engine tests covering file-only, syntax-only, and syntax-plus-semantic repositories
- Security: N/A
- Performance: validate query latency remains acceptable on syntax-indexed repositories

## Dependencies

- Requires Ticket 1
- Requires Ticket 2 for capability-tier model/reporting semantics
- Requires at least Ticket 3 and one language ticket to validate syntax-indexed behavior meaningfully

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [crates/query-engine/src/store_service.rs](crates/query-engine/src/store_service.rs)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)
- [crates/service/src/routes.rs](crates/service/src/routes.rs)

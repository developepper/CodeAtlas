## Problem

CodeAtlas still makes agents read too much raw code because exact source slices
are not first-class retrieval outputs. Whole-file retrieval is often broader
than necessary for symbol-level or workflow-level questions.

## Scope

- add exact slice retrieval primitives backed by persisted content
- support symbol-body, enclosing-scope, and explicit range retrieval in the
  store/query-engine layer
- define query-engine contracts for exact slices
- enforce size guardrails so slice retrieval remains context-efficient before
  external surfaces are added

## Deliverables

- slice retrieval implementation in store/query-engine
- query-engine contracts for exact slices
- tests for deterministic range correctness

## Acceptance Criteria

- [ ] the query layer can retrieve an exact source slice by stable line/byte range
- [ ] the query layer can retrieve a symbol body without loading the whole file
- [ ] slice results include enough metadata to explain what was returned
- [ ] slice results enforce explicit maximum size constraints

## Testing Requirements

- Unit: range normalization and guardrail logic
- Integration: store/query-engine exact-slice retrieval
- Regression: deterministic slice output across repeated calls
- Performance: slice retrieval avoids obvious overhead compared to whole-file reads

## Dependencies

- Requires the architecture ticket
- Can begin before broader graph work
- Service/MCP/CLI exposure is intentionally split into a follow-on ticket

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if user-facing behavior changes
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/planning/github-issues/file-indexing-ticket-query-content.md](docs/planning/github-issues/file-indexing-ticket-query-content.md)

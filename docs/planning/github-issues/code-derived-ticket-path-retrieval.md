## Problem

Agents often ask path-oriented questions such as "what writes this state?" or
"how does behavior move from A to B?", but CodeAtlas does not yet expose
generic workflow/path retrieval built on stored code evidence.

## Scope

- add generic path retrieval on top of persisted graph edges
- support entrypoint-to-writer and symbol-to-symbol pathfinding where evidence exists
- return minimal supporting evidence bundles rather than broad narrative summaries
- bound traversal so path queries remain explainable and do not imply coverage
  beyond the available edge set

## Deliverables

- path/workflow query implementation
- evidence bundle format for path answers
- representative integration coverage across more than one language ecosystem

## Acceptance Criteria

- [ ] a client can request a path between two code artifacts where supporting edges exist
- [ ] at least one query explains an entrypoint-to-writer style path using stored evidence
- [ ] path answers remain inspectable in terms of edges and slices used
- [ ] the query semantics stay structural rather than framework-specific
- [ ] path traversal has explicit limits or guardrails so sparse graphs do not
      yield misleading "best effort" paths without qualification
- [ ] missing-path or low-coverage situations are surfaced explicitly rather
      than presented as negative proof

## Testing Requirements

- Unit: path selection/traversal logic
- Integration: service/MCP/query-engine path queries on representative fixtures
- Diagnostics: missing-path and partial-coverage cases are coherent

## Dependencies

- Requires architecture ticket
- Requires exact-slice retrieval ticket
- Requires graph persistence ticket
- Should follow graph query surfaces ticket

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)

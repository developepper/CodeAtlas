## Problem

The index knows files and symbols, but it does not yet persist enough portable
relationship edges to support graph-aware retrieval across languages.

## Scope

- persist a language-agnostic relationship graph for the most portable edge kinds
- start with containment, imports/dependencies, and the most trustworthy
  reference/call edges currently derivable
- make explicit which relationships remain implicit in existing schema and do
  not need duplicate edge rows in the first slice
- define coverage reporting per language and edge kind
- keep partial coverage explicit rather than pretending all languages support
  the same edge depth

## Deliverables

- edge persistence model
- graph storage/query support
- coverage reporting for available edge kinds
- quality/coverage visibility for graph depth

## Acceptance Criteria

- [ ] persisted edges exist beyond relationships already trivially derivable
      from the existing schema
- [ ] edge records include kind, derivation method, and evidence/confidence
- [ ] partial per-language coverage is visible rather than hidden
- [ ] at least two edge kinds beyond containment are queryable in production paths
- [ ] graph coverage is visible through existing reporting/diagnostic surfaces

## Testing Requirements

- Unit: edge construction and evidence serialization
- Integration: indexing persists edges and retrieval returns them
- Diagnostics: coverage reporting is accurate and understandable

## Dependencies

- Requires the architecture ticket
- Should follow or overlap with exact-slice work, but before path-query work

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)

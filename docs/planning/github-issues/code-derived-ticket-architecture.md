## Problem

The proposed code-derived repository intelligence direction is still broad
enough that implementation could drift unless the graph model, slice model,
derived-fact model, and explanation requirements are made explicit up front.

## Scope

- define the architecture for code-derived repository intelligence
- define exact-slice retrieval contracts
- define relationship edge and derived-fact concepts
- define where relationship edges are produced in the indexing lifecycle
- distinguish implicit relationships already derivable from existing schema from
  explicit persisted graph edges
- define incremental invalidation/recompute handling for edges and facts
- define evidence and explanation requirements for higher-level retrieval
- define first-slice boundaries and migration stance

## Deliverables

- architecture/planning updates
- shared vocabulary for slices, edges, facts, and path queries
- implementation constraints for the first execution slice

## Acceptance Criteria

- [ ] the architecture defines exact slice retrieval as a first-class concept
- [ ] relationship edges and derived facts have explicit evidence requirements
- [ ] the architecture decides how edges are produced: backend-emitted,
      post-pass derived, or a hybrid model
- [ ] the architecture clarifies which relationships need explicit edge records
      versus which remain implicit in existing schema
- [ ] the architecture states how incremental indexing invalidates or recomputes
      affected edges and derived facts, even if the first answer is
      conservative/full-recompute
- [ ] workflow/path retrieval is defined as graph traversal, not free-form
      summarization
- [ ] the first execution slice is constrained enough to avoid framework lock-in
      or "AI understanding" scope creep

## Testing Requirements

- Docs review: planning and issue drafts remain internally consistent
- N/A for unit/integration/runtime in this ticket itself

## Dependencies

- Parent epic: proposed Code-Derived Repository Intelligence epic
- Follows Epic 17 as the substrate for broader syntax-backed retrieval

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Docs updated and internally consistent
- [ ] Architecture direction is reviewable before implementation tickets begin

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/engineering-principles.md](docs/engineering-principles.md)

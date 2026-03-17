# Code-Derived Repository Intelligence Issue Creation Order

This document defines the recommended issue creation and execution order for
the proposed code-derived repository intelligence epic.

It exists to keep the first slice disciplined and to prevent implementation
from starting before the architecture and evidence model are settled.

## Principles

- create the architecture gate first
- split primitive-building tickets from surface-exposure tickets
- build on small trustworthy primitives before higher-level facts and paths
- keep graph-dependent tickets behind explicit graph-quality gates
- allow parallel work only where contracts are already defined

## Recommended Issue Creation Order

1. Epic: Code-Derived Repository Intelligence
2. Ticket 1: Define the code-derived repository intelligence architecture
3. Ticket 2: Persist exact slice retrieval primitives in the store/query layer
4. Ticket 3: Expose exact slice retrieval through service, MCP, and CLI
5. Ticket 4: Add relationship graph persistence for language-agnostic edges
6. Ticket 5: Add graph-aware query surfaces
7. Ticket 6: Add conservative derived structural facts
8. Ticket 7: Add workflow/path retrieval
9. Ticket 8: Rework ranking and explanation metadata
10. Ticket 9: Benchmark context reduction and answer quality

## Dependency Order

### Hard gate

- Ticket 1 is the architecture gate.
- No implementation ticket should begin before Ticket 1 is reviewed.

### Primitive order

- Ticket 2 should begin before Ticket 3.
- Ticket 4 should not begin before Ticket 1 has decided the edge-production and
  invalidation model.
- Ticket 5 should not begin before Ticket 4 has landed enough persisted edges
  to support meaningful graph queries.

### Fact and path order

- Ticket 6 should not assume strong call/reference graph depth.
- Ticket 7 should not begin before Tickets 2 and 4 exist, and should preferably
  follow Ticket 5 so path traversal can reuse graph query contracts.
- Ticket 8 can begin once at least one graph-aware retrieval surface exists,
  but it should land after Tickets 6 and/or 7 so explanation metadata can be
  included where useful.

### Benchmark order

- Ticket 9 should land after:
  - Ticket 2 or Ticket 3 so exact-slice retrieval exists in a user-facing form
  - Ticket 5 or Ticket 7 so graph-assisted retrieval can be compared against
    search-only or whole-file retrieval

## Suggested Parallelism

Once Ticket 1 is complete:

- Ticket 2 can begin immediately.
- Ticket 4 can begin after the architecture resolves edge production and
  implicit-vs-explicit relationship handling.

Once Ticket 2 is complete:

- Ticket 3 can proceed in parallel with Ticket 4 if the slice contracts are
  stable.
- Ticket 3 should avoid locking in a special-case response shape that would make
  later graph-aware query surfaces feel inconsistent.

Once Ticket 4 is complete:

- Ticket 5 can begin.
- Ticket 6 can begin only if its first facts remain conservative and aligned to
  the currently available edge set.

Once Ticket 5 is complete:

- Ticket 7 can begin with clearer graph query semantics.
- Ticket 8 can begin if explanation payloads are scoped to already-existing
  query surfaces.

Ticket 9 should remain near the end because it depends on actual retrieval
behavior, not just planned contracts.

## Review Checklist For Issue Creation

Before creating GitHub issues, confirm:

- the epic body stays concise and points to the planning artifact
- Ticket 1 explicitly covers:
  - edge production strategy
  - implicit vs explicit relationship storage
  - incremental invalidation/recompute handling
- Ticket 2 and Ticket 3 remain split
- Ticket 3 surface design leaves room for later graph-aware query surfaces to
  follow similar contract patterns
- Ticket 6 stays conservative rather than assuming rich graph depth too early
- benchmark language remains honest about token wins being situational rather
  than universal

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/planning/github-issues/code-derived-repository-intelligence-epic.md](docs/planning/github-issues/code-derived-repository-intelligence-epic.md)

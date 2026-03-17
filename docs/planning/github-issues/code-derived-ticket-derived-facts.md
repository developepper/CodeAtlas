## Problem

Even with slices and graph queries, the system still rediscovers the same
higher-level structural observations unless those observations are persisted as
evidence-backed derived facts.

## Scope

- compute deterministic structural facts from indexed artifacts and edges
- start only with conservative facts such as central symbols,
  import/dependency fan-out, public-surface-style signals, and the strongest
  hotspot-style facts supportable by available edges
- expose facts as queryable artifacts with inspectable evidence payloads

Explicitly out of scope for this ticket:

- coordinator inference
- state-writer inference
- side-effect-boundary inference that depends on richer call/reference depth

Those should be deferred to a follow-on ticket once graph maturity is proven.

## Deliverables

- derived-fact model
- fact computation pipeline
- query surfaces for structural facts

## Acceptance Criteria

- [ ] at least one centrality-style fact is persisted and queryable
- [ ] at least one conservative non-call-graph fact is persisted and queryable
- [ ] every fact includes a reason payload and supporting evidence
- [ ] facts are deterministically reproducible from indexed artifacts
- [ ] no acceptance criterion for this ticket depends on dense call/reference
      graph coverage

## Testing Requirements

- Unit: fact computation logic
- Integration: fact persistence and retrieval
- Regression: repeated indexing yields stable fact outputs on unchanged repos

## Dependencies

- Requires architecture ticket
- Requires graph persistence ticket
- Richer call/reference-heavy facts should wait for a follow-on ticket after
  graph depth is proven in production paths

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/engineering-principles.md](docs/engineering-principles.md)

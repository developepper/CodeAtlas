## Problem

Richer retrieval will not feel meaningfully smarter if results are still ranked
mostly by shallow textual matching and cannot explain why a result or slice was
chosen.

## Scope

- rework ranking to use structural evidence and graph signals
- return explanation metadata for graph-aware query surfaces and any new
  slice/path/fact surfaces introduced by this epic
- keep explanations deterministic and inspectable rather than verbose or opaque

## Deliverables

- graph-aware ranking updates
- explanation payloads
- docs for ranking/explanation semantics

Minimum explanation payload shape:

- signals used for ranking or selection
- supporting edge ids where applicable
- supporting slice ids or slice bounds where applicable
- confidence or score contribution fields where practical

## Acceptance Criteria

- [ ] ranking can use structural evidence beyond plain text matching
- [ ] graph-aware query surfaces expose explanation metadata
- [ ] any new slice/path/fact surfaces added by this epic expose explanation
      metadata when ranking or selection is involved
- [ ] explanation payloads identify the signals, slices, edges, or facts that
      drove the result
- [ ] repeated runs remain deterministic for equivalent inputs

## Testing Requirements

- Unit: ranking and explanation construction
- Integration: representative graph/fact query ranking behavior
- Regression: deterministic result ordering and explanation output

## Dependencies

- Requires architecture ticket
- Requires graph persistence ticket
- Should follow or overlap with derived-facts and path-retrieval tickets

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/engineering-principles.md](docs/engineering-principles.md)

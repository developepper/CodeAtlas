## Problem

Repository-intelligence features are only valuable if they measurably reduce
context read or improve answer usefulness. Without benchmark guidance, the epic
can drift into richer metadata with no evidence of practical benefit.

## Scope

- define benchmark methodology for exact-slice and graph-assisted retrieval
- measure narrower retrieval against whole-file retrieval
- capture token/context avoided, answer usefulness, and any latency tradeoffs
- keep benchmark claims honest and ecosystem-agnostic
- require repository diversity sufficient to avoid single-repo benchmark stories

## Deliverables

- benchmark guidance update
- benchmark tasks and evaluation criteria
- explicit evidence expectations for context-reduction claims

## Acceptance Criteria

- [ ] benchmark guidance compares exact-slice retrieval against whole-file retrieval
- [ ] benchmark guidance compares graph-assisted retrieval against search-only retrieval
- [ ] benchmark output expectations include context avoided or token reduction evidence
- [ ] docs remain honest that richer retrieval may improve answer quality even when token wins are not universal
- [ ] benchmark guidance defines what counts as success: measurable improvement
      on at least one task family, or explicit documentation of tradeoffs when
      wins are not universal
- [ ] benchmark guidance requires more than one repository/language shape in
      evaluation (for example at least one repo with richer graph depth and one
      repo with shallower graph coverage)

## Testing Requirements

- Benchmark/manual validation: representative repository runs
- Docs validation: benchmark commands/examples remain consistent with shipped behavior

## Dependencies

- Requires architecture ticket
- Should land after slice retrieval plus at least one graph-aware query ticket

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Docs updated and internally consistent
- [ ] Benchmark guidance updated and reviewable
- [ ] CI green if docs/tests are affected

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/benchmarks/blog-benchmark-kit.md](docs/benchmarks/blog-benchmark-kit.md)

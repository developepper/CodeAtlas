## Problem

The benchmark and documentation model must evolve from file-only versus
semantic-heavy narratives into a layered story that measures file, syntax, and
semantic usefulness separately.

## Scope

- update docs for the new long-term architecture
- update benchmark guidance to measure file, syntax, and semantic coverage
- evaluate token/context reduction on the new syntax-indexed ecosystems

## Deliverables

- updated user-facing docs
- updated benchmark methodology
- token-efficiency comparison guidance for file-only versus syntax-indexed repos

## Acceptance Criteria

- [ ] docs explain syntax indexing as the normal baseline for major recognized languages
- [ ] benchmark guidance distinguishes file, syntax, and semantic coverage
- [ ] benchmark guidance includes Laravel/PHP proving-ground expectations
- [ ] docs remain honest about where semantic enrichment still exceeds syntax-only capability

## Testing Requirements

- Unit: N/A
- Integration: validate referenced commands/docs examples remain consistent with shipped CLI/query behavior
- Security: N/A
- Performance: benchmark guidance captures token/context and indexing-performance implications where relevant

## Dependencies

- Requires Ticket 1
- Should land after Tickets 2-4 so docs and benchmark guidance reflect real architecture and PHP proving-ground evidence

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Docs updated and internally consistent
- [ ] Benchmark guidance updated and reviewable
- [ ] CI green if docs/tests are affected

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [README.md](README.md)
- [docs/benchmarks/blog-benchmark-kit.md](docs/benchmarks/blog-benchmark-kit.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)

## Problem

Current docs and quality terminology center on symbol extraction, which will be
misleading once file-level indexing becomes a first-class baseline.

## Scope

- update README and runbook guidance
- update quality-report wording or semantics as needed
- update benchmark and blog guidance so file-level coverage can be measured
- keep docs explicit about file-level indexing versus symbol-level indexing

## Deliverables

- updated user-facing docs
- updated operator-facing docs
- updated benchmark/blog guidance

## Acceptance Criteria

- [ ] docs explain that recognized files are indexed even when symbols are unavailable
- [ ] docs explain which query surfaces remain useful for file-only indexed repos
- [ ] quality-report wording does not imply that zero symbols means zero index value
- [ ] benchmark guidance includes file-level coverage as a first-class metric
- [ ] docs remain honest about current symbol coverage limits by language

## Testing Requirements

- Unit: not required
- Integration: not required
- Review: manual doc review for accuracy against implementation

## Dependencies

- Parent epic: TBD
- Depends on: pipeline/store ticket (TBD)
- Depends on: query/content ticket (TBD)

## Review Checklist

- user journey is understandable for file-only indexed repos
- no doc implies unsupported symbol coverage exists today
- metrics language matches product behavior
- benchmark/blog guidance can demonstrate the new baseline clearly

## References

- [docs/planning/recognized-language-file-indexing.md](docs/planning/recognized-language-file-indexing.md)
- [README.md](README.md)
- [docs/operations/runbook.md](docs/operations/runbook.md)
- [docs/benchmarks/blog-benchmark-kit.md](docs/benchmarks/blog-benchmark-kit.md)
- GitHub issue: TBD

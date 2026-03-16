## Objective

Make syntax indexing the default baseline for major recognized code languages
in CodeAtlas, with semantic indexing layered on top and file-only indexing
reserved as the explicit last fallback.

## Problem

CodeAtlas now has a solid file-level baseline for recognized languages, but
symbol-bearing indexing remains sparse. Many high-value ecosystems such as PHP,
Python, Go, Java, and JavaScript still fall back to file-only indexing, which
limits symbol search, file outline usefulness, and token reduction for AI
workflows.

The product needs a broader syntax indexing platform rather than continuing to
grow a small set of language-specific special cases.

## In Scope

- define the long-term syntax/semantic/file capability architecture
- refactor current adapter/routing assumptions where needed
- create a multi-language tree-sitter-backed syntax subsystem
- evolve core model, metrics, and query expectations for capability tiers
- implement the first wave of production-grade syntax extraction on the new
  platform
- update docs and benchmark guidance for file, syntax, and semantic coverage

## Out Of Scope

- preserving existing interfaces purely for backward compatibility
- hosted deployment concerns unrelated to indexing architecture
- broad semantic parity across all languages in the first slice
- unrelated query-surface expansion not needed for syntax-first workflows

## Child Tickets

- TBD Ticket: Define the universal syntax indexing architecture and capability model
- TBD Ticket: Refactor core model and metrics for file/syntax/semantic capability tiers
- TBD Ticket: Create the multi-language syntax indexing subsystem and migrate Rust onto it
- TBD Ticket: Implement PHP syntax indexing on the new subsystem
- TBD Ticket: Implement Python syntax indexing
- TBD Ticket: Implement Go syntax indexing
- TBD Ticket: Implement Java syntax indexing
- TBD Ticket: Implement JavaScript syntax indexing
- TBD Ticket: Rework query surfaces for broad syntax coverage
- TBD Ticket: Update benchmark and token-efficiency evaluation strategy

## Epic Definition Of Done

- syntax indexing is the normal baseline for major recognized code languages
- PHP/Laravel is no longer file-only
- file-only indexing remains available but is no longer the expected outcome
  for most common code repositories
- metrics, docs, and benchmarks clearly distinguish file, syntax, and semantic
  coverage
- the architecture is positioned for continued language expansion without
  repeated foundational redesign

## Review Evidence Required

- architecture/planning doc updates merged
- explicit note that clean long-term architecture is favored over backward
  compatibility for this initiative
- acceptance evidence per child ticket
- Laravel/PHP proving-ground evidence
- evidence that syntax indexing meaningfully improves query usefulness beyond
  file-only behavior

## Notes

- This epic is intentionally larger than prior slices because it establishes a
  new platform baseline rather than a narrow feature.
- The preferred syntax substrate is tree-sitter.
- Semantic adapters remain important, but they are treated as enrichment over a
  syntax baseline rather than the only path to useful symbol coverage.

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/planning/issue-backlog.md](docs/planning/issue-backlog.md)

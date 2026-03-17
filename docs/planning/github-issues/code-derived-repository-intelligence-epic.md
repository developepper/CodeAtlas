## Objective

Make CodeAtlas feel deeply knowledgeable about a repository based on the code
itself, by persisting exact slices, relationship edges, derived structural
facts, and workflow/path evidence rather than relying on broad file reads or
human-authored descriptions.

## Problem

CodeAtlas has stronger syntax coverage after Epic 17, but retrieval is still
too dependent on outlines, whole-file reads, and repeated session-by-session
rediscovery. The next step is a code-derived repository intelligence layer that
is driven by exact slices, persisted relationship edges, conservative derived
facts, and inspectable path evidence.

## In Scope

- exact symbol/file slice retrieval
- persistence of language-agnostic relationship edges
- graph-aware query APIs
- derived structural facts from source/index artifacts
- generic workflow/path retrieval
- ranking and explanation improvements driven by structural evidence
- benchmarks for context reduction and answer quality

## Out Of Scope

- framework-specific enrichments as the primary execution path
- issue/PR/commit-message learning
- hosted/team features unrelated to retrieval quality
- broad semantic parity across all languages in the first slice

## Child Tickets

- Ticket 1: Define the code-derived repository intelligence architecture
- Ticket 2: Persist exact slice retrieval primitives in the store/query layer
- Ticket 3: Expose exact slice retrieval through service, MCP, and CLI
- Ticket 4: Add relationship graph persistence for language-agnostic edges
- Ticket 5: Add graph-aware query surfaces
- Ticket 6: Add conservative derived structural facts
- Ticket 7: Add workflow/path retrieval
- Ticket 8: Rework ranking and explanation metadata
- Ticket 9: Benchmark context reduction and answer quality

## First Slice Guardrails

- build upward from exact slices and trustworthy persisted edges
- prefer portable graph primitives over framework-specific inference
- keep derived facts evidence-backed and inspectable
- do not claim universal token wins; benchmark against narrower retrieval goals

## Epic Definition Of Done

- exact code slices can be retrieved by stable ranges for common workflows
- relationship edges beyond simple symbol lists are persisted and queryable
- code-derived structural facts are available as tested retrieval artifacts
- at least one workflow/path query works from code-derived evidence alone
- benchmark evidence shows more targeted context use or better answer quality
- the design remains broadly useful across languages and project structures

## Review Evidence Required

- architecture/planning doc updates merged
- explicit evidence that the new knowledge is derived from code/index artifacts
- retrieval examples where exact slices replace broader file reads
- graph/path evidence across more than one language ecosystem
- benchmark evidence for improved context efficiency or retrieval usefulness
- inspectable evidence payloads for facts, paths, or rankings

## Notes

- This epic should follow Epic 17 and build on the broader syntax baseline it
  established.
- The primary value is code-derived understanding, not human-authored memory.
- The preferred early direction is strongly ecosystem-agnostic; framework
  enrichments can layer on later if needed.
- The planning artifact should remain the canonical detailed source; this epic
  body is intentionally concise.
- The recommended sequencing and dependency order live in the issue-creation
  order doc referenced below.

## References

- [docs/planning/code-derived-repository-intelligence.md](docs/planning/code-derived-repository-intelligence.md)
- [docs/architecture/code-derived-repository-intelligence-notes.md](docs/architecture/code-derived-repository-intelligence-notes.md)
- [docs/planning/github-issues/code-derived-issue-creation-order.md](docs/planning/github-issues/code-derived-issue-creation-order.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [docs/architecture/universal-syntax-indexing-architecture.md](docs/architecture/universal-syntax-indexing-architecture.md)
- [docs/engineering-principles.md](docs/engineering-principles.md)

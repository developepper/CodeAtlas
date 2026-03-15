## Objective

Make CodeAtlas produce a useful, non-empty index for repositories containing
recognized languages even when symbol adapters are missing or incomplete.

## Problem

Today, CodeAtlas recognizes many languages during discovery but only persists
useful index artifacts when an adapter successfully extracts symbols. Files in
recognized languages without a working adapter disappear from the index, which
makes file tree, repo outline, and file-content retrieval ineffective on many
real repositories.

## In Scope

- define the recognized-file indexing contract
- persist file records and blobs for recognized files even without symbols
- distinguish missing-adapter paths from real adapter failures
- wire file-content retrieval to stored blobs
- update metrics and docs so file-level indexing is a first-class baseline

## Out Of Scope

- broad multi-language grammar implementation
- semantic adapter expansion itself
- hosted storage redesign
- unrelated query-surface expansion

## Child Tickets

- TBD Ticket: Persist file records and blobs for recognized files without symbol adapters
- TBD Ticket: Wire file-content retrieval and file-level query behavior to the fallback model
- TBD Ticket: Update metrics, docs, and benchmark guidance for file-level indexing coverage

## Epic Definition Of Done

- a recognized-language repository no longer collapses to an empty index solely
  because symbol adapters are unavailable
- file tree and repo outline show recognized indexed files regardless of symbol
  availability
- file content is retrievable for indexed recognized files
- symbol-bearing languages continue to produce symbols as before
- docs and metrics explain the file-level baseline clearly

## Review Evidence Required

- architecture/planning doc updates merged
- acceptance evidence per child ticket
- end-to-end proof on a recognized-language repo with no current symbol adapter
- explicit note on missing-adapter versus adapter-failure behavior
- explicit note on file-content retrieval wiring

## Notes

- This issue is intended to establish the minimum useful indexing baseline for
  recognized languages.
- New tree-sitter grammars and semantic adapters remain additive follow-on
  work, not prerequisites for this epic.

## References

- [docs/planning/recognized-language-file-indexing.md](docs/planning/recognized-language-file-indexing.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/planning/issue-backlog.md](docs/planning/issue-backlog.md)
- [docs/benchmarks/blog-benchmark-kit.md](docs/benchmarks/blog-benchmark-kit.md)

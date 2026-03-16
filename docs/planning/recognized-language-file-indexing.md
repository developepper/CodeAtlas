# Recognized-Language File Indexing Plan

Status: Complete — all tickets merged (#166, #167, #165)

Follow-on architecture work now lives in:
`docs/planning/universal-syntax-indexing.md`

Owner intent: ensure CodeAtlas produces a useful, non-empty index for any
repository containing files in recognized languages, even when symbol adapters
are unavailable or incomplete.

## Why This Exists

Today, CodeAtlas recognizes many languages during discovery but only produces
persisted index artifacts when an adapter successfully parses the file.

Current consequence:

- recognized files without a registered adapter are treated as parse errors
- those files do not receive file records
- blob content is not persisted for those files
- file tree, file outline, and repo outline queries have nothing to show
- some repositories end up effectively empty even though language detection
  found many real source files

That undermines the core product promise: giving AI clients a structured,
token-efficient way to navigate a repository without pasting large raw code
blobs into prompts.

Files with `Language::Unknown` remain out of scope for this initiative and
should continue to be excluded at discovery time. This work only changes the
behavior for files whose language detection result is recognized and stable.

## Product Decision

Every discovered file with a recognized language should produce a durable index
artifact.

The quality of that artifact should scale with adapter capability:

1. **File-level indexing baseline**
   - every recognized file gets a file record and stored content blob
   - the file appears in file tree and repo outline queries
   - the file is eligible for content retrieval
   - the file may have zero symbols

2. **Symbol-level indexing when adapters succeed**
   - languages with working syntax or semantic adapters additionally produce
     symbol records
   - file outline includes extracted symbols when available
   - symbol search and exact symbol lookup work as they do today

This initiative does **not** require immediate multi-language symbol parity.
It establishes a non-empty baseline and clear capability layering.

## User Workflows To Support

### Workflow A: Unsupported-but-recognized language repo

1. User indexes a repository containing Python, Go, Java, or another
   recognized language with no current tree-sitter adapter.
2. CodeAtlas persists file records and blobs for recognized files.
3. The user can browse the file tree, inspect repo outline, and retrieve file
   content through CLI or AI tooling.
4. Symbol search returns little or nothing, but the repo is still navigable.

### Workflow B: Mixed-capability repo

1. User indexes a mixed Rust/TypeScript/Markdown/SQL repository.
2. Rust and configured TypeScript files produce symbols.
3. Markdown and SQL files still appear as indexed files with retrievable
   content.
4. The user gets a partially structured index rather than an empty or
   misleadingly sparse one.

### Workflow C: Adapter failure on a recognized file

1. User indexes a repo where an adapter exists but one file fails parsing.
2. The file still receives a file record and stored content.
3. The index surfaces the file as present but without symbols.
4. Diagnostics distinguish "file indexed without symbols" from "symbol
   extraction failed."

## Key Product Decisions

### 1. "No adapter registered" is not an indexing error

When language detection succeeds but the router returns no adapter, that should
be treated as an expected capability boundary, not a fatal or file-dropping
error path.

### 2. File-level indexing is part of the core contract

The index should be useful at the file layer even when symbol extraction is
not available. This is required for:

- file tree navigation
- repo outline visibility
- AI prompt compression through targeted file retrieval

### 3. Adapter failure and adapter absence are different states

The implementation must distinguish:

- no adapter exists for the language
- adapter exists but returned `Unsupported`
- adapter existed and failed with a real error

These states may produce the same user-visible fallback artifact (file record +
blob, zero symbols), but they should not collapse into the same internal or
diagnostic meaning.

### 4. File content retrieval must become real, not placeholder

The current query layer returns placeholder empty content for indexed files.
That is incompatible with the product goal for file-level fallback. This
initiative includes wiring file-content retrieval to blob storage in the
production query/service path.

### 5. Symbol coverage remains additive follow-on work

Adding new tree-sitter grammars or semantic adapters later should move
languages from file-level only to file-plus-symbol indexing without changing
the baseline contract.

### 6. File-only records use the same content hash contract

File-only indexed records should use the same `store::content_hash` SHA-256
content hash as symbol-bearing files. This keeps blob deduplication and file
record semantics consistent across both indexing modes.

## Recommended Architecture Direction

### Parsing model

The parse stage should produce two classes of successful artifacts:

- **symbol-bearing parsed files**
- **file-only parsed files**

Either representation is acceptable as long as the persist stage can write file
records and blobs for all recognized files, not just files with symbols.

Implementation note:

- the current `merge_outputs` contract returns `None` for an empty output list
  and therefore cannot be the only gate for whether a recognized file survives
  indexing
- the implementation must either bypass merge for file-only cases or make the
  no-adapter / no-symbol path explicit before merge is consulted

### Persistence model

The persist stage should:

- write blobs for all recognized files that survive discovery
- write file records for all recognized files
- write symbol records only for files with adapter output
- preserve stale-file cleanup based on discovery output

### Query model

The query layer should support:

- file tree over all indexed recognized files
- file outline returning file metadata plus zero or more symbols
- real file content retrieval from blob storage
- repo outline counts reflecting file-only indexed files as first-class
  members of the repo

### Metrics and diagnostics model

The product should stop equating "no symbol adapter" with "file errored."

Metrics should distinguish:

- discovered recognized files
- file-level indexed files
- symbol-bearing indexed files
- true adapter failures

Quality reporting should stay honest that symbol quality is separate from
file-level index coverage.

## What Must Change

### Current behavior

- recognized languages are discovered
- the parse stage records `no adapter available` as a file error
- the persist stage only writes file records for successfully parsed files
- `get_file_content` returns an empty placeholder
- files with `Language::Unknown` are excluded during discovery

### Target behavior

- every recognized file yields a file record and stored blob
- missing adapters do not make the file disappear from the index
- real adapter failures do not make the file disappear from the index
- queries can retrieve file content and browse file structure even without
  symbols

## Scope

### In scope

- define the canonical indexing contract for recognized files
- change pipeline behavior so recognized files persist even without symbols
- wire file-content retrieval to blob storage
- update metrics, quality-report semantics, and docs
- add tests proving non-empty index behavior on recognized non-Rust repos

### Out of scope

- broad new tree-sitter grammar implementation across many languages
- semantic adapter expansion itself
- hosted/remote content storage
- UI work beyond current CLI/MCP/query surfaces

## Resolved Design Decisions

1. **Per-file status metadata**
   - not required in the first slice
   - existing diagnostics and repo-level metrics should distinguish file-only
     fallback from true adapter failure
   - per-file status metadata can be a follow-up once the baseline behavior is
     proven

2. **Metric terminology**
   - `files_parsed` should continue to mean files that produced symbol-bearing
     adapter output unless implementation pressure makes that actively
     misleading
   - this slice should introduce or document a separate file-level indexing
     metric rather than redefining `files_parsed` silently

3. **Blob-backed content retrieval boundary**
   - file-content retrieval should be wired in the production query/service
     path as part of this initiative
   - placeholder content is not acceptable once file-level indexing is a
     product contract

4. **Quality-report interpretation**
   - quality-report should remain honest that symbol quality and file-level
     index coverage are different measures
   - zero symbols must not imply "zero index value" when file-level indexing is
     present

## Issue Breakdown

All issues created and merged:

- ~~#164 Epic 16: Non-Empty Index Baseline For Recognized Files~~
- ~~#166 Ticket 1: Persist file records and blobs for recognized files without symbol adapters~~
- ~~#167 Ticket 2: Wire file-content retrieval and file-level query behavior to the fallback model~~
- ~~#165 Ticket 3: Update metrics, docs, and benchmark guidance for file-level indexing coverage~~

## Epic Draft

### Objective

Make CodeAtlas produce a useful, non-empty index for repositories containing
recognized languages even when symbol adapters are missing or incomplete.

### Problem

The current parse and persist model drops recognized files unless an adapter
successfully produces symbols. This makes many non-Rust or mixed-language
repositories effectively unindexable and prevents AI clients from using
CodeAtlas as a file-level navigation and retrieval layer.

### In Scope

- define the recognized-file indexing contract
- persist file records and blobs for recognized files without symbols
- distinguish missing-adapter paths from real adapter failures
- wire file-content retrieval to stored blobs
- update docs and metrics to reflect file-level baseline indexing

### Out Of Scope

- large-scale new language grammar work
- hosted storage redesign
- unrelated query-surface expansion

### Epic Definition Of Done

- a recognized-language repository no longer collapses to an empty index solely
  because symbol adapters are unavailable
- file tree and repo outline show recognized indexed files regardless of symbol
  availability
- file content is retrievable for indexed recognized files
- symbol-bearing languages continue to produce symbols as before
- docs and metrics explain the file-level baseline clearly

## Ticket Drafts

### Ticket 1: Persist file records and blobs for recognized files without symbol adapters

Problem:

The parse stage currently records `no adapter available` as an error and the
persist stage only writes file records for parsed files with adapter output.

Scope:

- change parse/persist flow so recognized files survive without symbols
- preserve blob writes and file records for recognized files
- keep true adapter failures distinguishable in diagnostics
- use the same `store::content_hash` content-hash contract for file-only and
  symbol-bearing files
- handle the current `merge_outputs(vec![]) -> None` behavior intentionally so
  recognized files do not disappear through the empty-merge path
- add integration coverage for recognized non-Rust repositories

Deliverables:

- pipeline changes
- store/persist changes
- regression tests covering file-only indexing

### Ticket 2: Wire file-content retrieval and file-level query behavior to the fallback model

Problem:

Even when file records exist, `get_file_content` currently returns placeholder
empty content and the query layer is not designed around file-only indexed
artifacts as a primary use case.

Scope:

- wire file content retrieval to blob storage
- ensure file tree, file outline, and repo outline work for file-only indexed
  files
- validate service and MCP behavior for file-level-only repos

Deliverables:

- query/service wiring for content retrieval
- integration tests for CLI/service/MCP file-level queries

### Ticket 3: Update metrics, docs, and benchmark guidance for file-level indexing coverage

Problem:

Current docs and quality terminology center on symbol extraction, which will be
misleading once file-level indexing becomes a first-class baseline.

Scope:

- update README and runbook guidance
- update quality-report wording or semantics as needed
- update benchmark/blog guidance so file-level coverage can be measured

Deliverables:

- user-facing doc updates
- operator-facing doc updates
- benchmark guidance updates

## References

- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)
- [docs/planning/issue-backlog.md](docs/planning/issue-backlog.md)
- [docs/benchmarks/blog-benchmark-kit.md](docs/benchmarks/blog-benchmark-kit.md)
- [crates/indexer/src/stage.rs](crates/indexer/src/stage.rs)
- [crates/query-engine/src/store_service.rs](crates/query-engine/src/store_service.rs)
- [crates/cli/src/router.rs](crates/cli/src/router.rs)

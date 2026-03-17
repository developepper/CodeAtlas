# Code-Derived Repository Intelligence

Status: Proposed post-Epic-17 planning artifact

## Purpose

Make CodeAtlas feel deeply knowledgeable about a repository based on the code
itself, not on commit messages, issue history, or human-authored descriptions.

The goal is to move from "good structural lookup" toward "durable code-derived
understanding" by persisting executable truths about how a codebase is wired,
what symbols depend on each other, where behavior starts, where state changes,
and which code slices are actually necessary to answer a question.

This initiative is intentionally ecosystem-agnostic. It should improve
retrieval quality across many repositories and language stacks before any
framework-specific enrichment layers are considered.

## Goals

- derive more useful repository knowledge from source code alone
- reduce the amount of raw file content an AI client must read to answer
  cross-file questions
- make retrieval feel more "intimate" with a codebase by persisting graph
  structure, executable flows, and derived facts rather than rediscovering them
  each session
- improve token efficiency by retrieving exact slices and graph-near context
  instead of whole files whenever possible
- keep the design broadly useful across languages and repository styles rather
  than optimizing first for one framework or ecosystem

## Working Definitions

To keep this epic concrete, these terms should be interpreted narrowly:

- **Exact slice**: a source retrieval unit defined by stable byte and/or line
  bounds, small enough to include only the implementation or supporting context
  needed for a question
- **Relationship edge**: a persisted, source-backed connection between code
  artifacts such as containment, import, dependency, reference, call,
  implementation, or override
- **Derived structural fact**: a deterministic summary computed from indexed
  code artifacts and graph signals, such as "likely coordinator" or "write
  hotspot," always backed by inspectable evidence
- **Workflow/path query**: a query that traverses persisted edges to explain
  how behavior moves between code artifacts; it is not a free-form natural
  language summary layer
- **Repository intelligence**: stored, queryable structural knowledge derived
  from code and index artifacts, not from commit history, issue text, or human
  prose

## Why This Exists

CodeAtlas now has stronger syntax coverage after Epic 17, but most retrieval is
still shaped around symbols, files, outlines, and basic ranking. That is
useful, but it still leaves important gaps:

- agents often need to read entire files because the system cannot yet return
  only the exact implementation slice they need
- the index knows symbols exist, but not enough about how they relate to each
  other across calls, references, modules, and write paths
- broad questions still trigger wide search and repeated rediscovery because
  higher-level code facts are not persisted
- token savings remain inconsistent because retrieval is not yet optimized
  around minimal structural evidence

The next quality leap should come from better code-derived intelligence, not
from trusting descriptions that may be incomplete or wrong.

## Product Direction

CodeAtlas should evolve from a symbol/file index into a code-derived repository
intelligence layer with five broad capabilities:

1. **Exact structural retrieval**
   - retrieve exact symbol bodies and nearby supporting context by stable
     byte/line spans
   - avoid loading full files when a small slice is enough

2. **Relationship graph persistence**
   - persist references, calls, implementation edges, containment, imports, and
     module dependencies where derivable
   - make graph edges queryable rather than forcing every session to infer them

3. **Derived code facts**
   - infer write hotspots, read hotspots, central modules, likely coordinators,
     likely side-effect boundaries, and other structural facts from code
   - store those facts as evidence-backed index artifacts

4. **Workflow/path extraction**
   - answer "how does behavior move through this repo?" using entrypoints,
     coordinators, state writers, and async/external boundaries
   - do this in a language-agnostic way rather than through framework-specific
     assumptions

5. **Retrieval ranking and explanation**
   - rank results using graph centrality, structural proximity, write-path
     relevance, and evidence confidence
   - explain why a result or slice was selected

## Core Principles

### 1. Code first, prose second

The system should prefer concrete evidence derivable from source code and
persisted index artifacts over docs, PR text, issue bodies, or commit messages.

Human-authored descriptions may exist later as optional enrichment, but they
should not be required to achieve the primary value of this epic.

### 2. Agnostic by default

The default design should work across many languages and project types. Early
execution slices should prioritize abstractions that transfer well:

- exact slices
- references
- calls
- containment
- dependency edges
- workflow/path queries
- code-derived centrality and hotspot facts

Framework-specific enrichers can remain follow-on work.

### 3. Retrieval should minimize context, not merely organize it

The index should not only make the code easier to navigate; it should help an
agent read materially less code while still answering correctly.

That means exact-slice retrieval, graph-near supporting context, and ranking
that favors canonical execution paths over incidental references.

### 4. Persist durable facts, not session guesses

If the system repeatedly infers that:

- symbol A is a central coordinator
- symbol B is the main writer of a state transition
- module C has unusually high fan-in

then that information should become a persisted, explainable artifact rather
than remaining a one-off session inference.

### 5. Preserve determinism and explainability

Any new graph edges, rankings, or derived facts should be:

- reproducible
- source-backed
- confidence-scored where appropriate
- inspectable by users and tests

### 6. Build upward from small trustworthy primitives

The epic should not begin with ambitious "understanding" features that depend
on too many uncertain inferences at once.

Preferred layering:

1. exact slices
2. persisted edges
3. graph-aware queries
4. derived structural facts
5. workflow/path queries built on those primitives

This order is intentional. Higher-level repository intelligence should be the
product of lower-level evidence, not a substitute for it.

## In Scope

- exact symbol/file slice retrieval using stable spans
- persistence of language-agnostic relationship edges where derivable
- graph-aware retrieval and query APIs
- derived structural facts computed from index artifacts
- generic workflow/path queries based on graph structure
- ranking improvements driven by structural evidence
- retrieval explanation metadata
- benchmark guidance that measures context avoided and retrieval usefulness

## Out Of Scope

- framework-specific enrichment as the primary execution path
- issue/PR/commit-message learning
- speculative architectural summaries not grounded in code
- full semantic parity across all languages in the first slice
- hosted/team features unrelated to retrieval quality
- opaque machine-generated summaries with no evidence links

## First Execution Slice Constraints

To keep the first slice sharp and portable, it should satisfy all of the
following constraints:

- exact slice retrieval must land before any broad attempt at workflow/path
  explanations
- relationship persistence should begin with edges that transfer well across
  languages: containment, imports/dependencies, and the most trustworthy
  reference/call edges currently derivable
- derived structural facts should remain conservative and evidence-backed rather
  than trying to infer deep domain semantics
- benchmark work should compare narrower retrieval against whole-file retrieval,
  not attempt to prove universal wins on every question type
- no ticket should depend on framework-specific conventions to show value in
  the first execution slice

## Proposed Capability Areas

### A. Exact Slice Retrieval

Add first-class retrieval for:

- exact symbol body
- symbol plus enclosing scope
- explicit file line/byte range
- minimal supporting context around a symbol or callsite
- multi-slice bundles for small workflow explanations

This is the clearest near-term improvement for both token savings and answer
quality.

### B. Relationship Graph

Persist generic graph edges such as:

- contains / contained_by
- references
- calls / called_by
- implements / implemented_by
- overrides / overridden_by
- imports / imported_by
- module dependency edges

Not every language will support every edge at first. The graph model should
support partial coverage cleanly.

Recommended evidence requirements per edge:

- source artifact on both ends
- edge kind
- derivation method
- confidence where derivation is heuristic rather than exact
- optional supporting spans or symbol IDs

### C. Derived Structural Facts

Compute and persist facts such as:

- central symbols by fan-in/fan-out
- likely coordinators
- likely write hotspots
- likely side-effect boundaries
- public API surface vs internal helpers
- tightly coupled modules

These should be evidence-backed rather than generated as free-form prose.

Minimum expectations for any persisted fact:

- deterministic recomputation from indexed artifacts
- explicit fact kind
- supporting symbols/files/edges
- confidence or strength score when ranking is involved
- a queryable reason payload that explains why the fact exists

### D. Workflow And Path Queries

Support generic questions like:

- what starts this behavior?
- what writes this state?
- what depends on this module?
- what is the shortest meaningful path from symbol A to symbol B?
- which symbols are central to this workflow?

The emphasis is structural pathfinding, not framework-specific business
semantics.

Important constraint:

- "workflow" here means a path through stored code artifacts and edges
- it does not mean a generated product narrative or business-process summary
- any answer should remain inspectable in terms of the edges and slices used

### E. Retrieval Ranking And Explanation

Improve ranking using:

- structural centrality
- graph distance to known anchors
- write-path importance
- capability-tier confidence
- symbol prominence within module boundaries

Return explanation metadata so clients can understand why a result ranked
highly or why a slice was chosen.

## Suggested Execution Order

### Ticket 1: Define the code-derived repository intelligence architecture

- graph model
- slice retrieval contracts
- derived-fact model
- ranking/explanation model
- edge production strategy
- implicit-vs-explicit relationship model
- incremental invalidation/recompute stance
- compatibility and migration stance

### Ticket 2: Persist exact slice retrieval primitives in the store/query layer

- stable byte/line slice retrieval from stored content
- query-engine contracts for exact slices
- deterministic tests for range correctness
- explicit guardrails on maximum returned bytes/lines per slice response

### Ticket 3: Expose exact slice retrieval through service, MCP, and CLI

- service HTTP surface for exact slices
- MCP and CLI contracts for exact slices
- user-facing docs and integration coverage
- should not begin before Ticket 2 contracts are stable

### Ticket 4: Add relationship graph persistence for language-agnostic edges

- containment
- imports/dependencies
- generic references/calls where supported
- storage/query model updates
- coverage reporting so users know which edge kinds are available per language
- should not begin before Ticket 1 resolves edge production and invalidation stance

### Ticket 5: Add graph-aware query surfaces

- callers/callees
- references
- dependency neighbors
- central-symbol / related-symbol queries
- depends on persisted edge coverage from Ticket 4

### Ticket 6: Add conservative derived structural facts

- centrality from available edges
- import/dependency fan-out facts
- conservative public-surface or hotspot-style facts
- should remain limited to conservative facts that do not require dense
  call/reference graph coverage

Follow-on note:

- coordinator/state-writer/side-effect-boundary fact families should be treated
  as a later extension once graph depth and quality are proven strong enough to
  support them

### Ticket 7: Add workflow/path retrieval

- entrypoint-to-write paths
- symbol-to-symbol pathfinding
- minimal structural evidence bundles for behavior explanation
- should build on Tickets 2, 4, and preferably 5

### Ticket 8: Rework ranking and explanation metadata

- graph-aware ranking
- retrieval rationale
- structural confidence reporting
- explanation payloads that identify the slices, edges, and facts that drove
  the result
- should align to the query surfaces that already exist rather than inventing
  explanation shapes prematurely

### Ticket 9: Benchmark context reduction and answer quality

- measure exact-slice vs whole-file retrieval
- measure graph-assisted vs search-only workflows
- capture token/context avoided and answer usefulness
- should land after at least one user-facing slice surface and one graph-aware
  retrieval surface exist

## Definition Of Done For The Epic

- agents can request exact code slices instead of full files for common
  repository-intelligence workflows
- CodeAtlas persists and exposes useful relationship edges beyond simple symbol
  lists
- code-derived structural facts are queryable and tested
- at least one workflow/path query demonstrates better repository reasoning
  from code alone
- benchmark evidence shows improved context efficiency or answer quality on
  representative repositories
- the architecture remains broadly applicable across languages and project
  styles

## Review Evidence Required

- architecture/planning doc updates merged
- explicit evidence that new capabilities are derived from code/index artifacts,
  not external prose
- retrieval examples showing smaller exact slices replacing broader file reads
- evidence that at least some graph/path queries work across multiple language
  ecosystems
- benchmark evidence that context use becomes more targeted or efficient
- evidence payloads are inspectable enough that a human can understand where a
  fact, path, or result came from

## Key Risks

- **Scope diffusion**: "repository intelligence" can become an umbrella for too
  many unrelated ideas unless the primitive-first layering is enforced.
- **Weak edges**: low-confidence relationship extraction can make graph queries
  feel impressive but unreliable.
- **Token regressions**: richer retrieval can increase token use if slice and
  ranking constraints are not enforced.
- **Language skew**: one ecosystem can dominate the design if portability is
  not treated as a hard requirement.

Preferred mitigations:

- require evidence-backed contracts for each new primitive
- ship coverage reporting with graph features
- benchmark narrow retrieval against broader retrieval continuously
- prefer smaller, more trustworthy edge sets over broader but noisy coverage

## Notes

- This epic should build directly on Epic 17 rather than replacing it. Broad
  syntax coverage is the substrate that makes repository-intelligence features
  more widely available.
- Framework-specific enrichments can still be valuable later, but they should
  layer on top of the generic graph/slice/retrieval model rather than defining
  it.
- Correctness and explainability should beat breadth in the early execution
  slices. A smaller set of trustworthy graph edges is better than a broad set
  of poorly-grounded relationships.

## References

- [code-derived-repository-intelligence-notes.md](../architecture/code-derived-repository-intelligence-notes.md)
- [post-v1-roadmap.md](post-v1-roadmap.md)
- [universal-syntax-indexing.md](universal-syntax-indexing.md)
- [universal-syntax-indexing-architecture.md](../architecture/universal-syntax-indexing-architecture.md)
- [engineering-principles.md](../engineering-principles.md)
- [code-derived-issue-creation-order.md](github-issues/code-derived-issue-creation-order.md)

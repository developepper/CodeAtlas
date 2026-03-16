# Universal Syntax Indexing Platform Plan

Status: Active — planning baseline for Epic 17

Architecture status: ratified via Ticket 1 (#174) in
`docs/architecture/universal-syntax-indexing-architecture.md`

Owner intent: evolve CodeAtlas from a narrow adapter set plus file-level
fallback into a general-purpose code intelligence platform where syntax
indexing is the default baseline for most recognized code languages and
semantic indexing is an enrichment layer on top of that baseline.

Canonical technical design source:
`docs/architecture/universal-syntax-indexing-architecture.md`

## Why This Exists

CodeAtlas now has a solid file-level baseline for recognized languages, but
symbol-bearing indexing remains sparse:

- Rust has syntax indexing
- TypeScript and Kotlin have semantic indexing
- many other recognized code languages fall back to file-only indexing

That is a reasonable transitional state, but it is not the right long-term
architecture for the product.

For the product vision, file-only indexing should be the exception rather than
the default outcome for most recognized source languages. Broad syntax indexing
must become the standard baseline capability so AI clients can navigate code by
symbol, not only by file.

## Product Direction

CodeAtlas should adopt a three-layer capability model:

1. **File layer**
   - file record
   - content blob
   - file tree / repo outline visibility
   - exact file-content retrieval

2. **Syntax layer**
   - broad multi-language symbol extraction
   - stable spans and symbol IDs
   - file outline and symbol search usable across most recognized languages

3. **Semantic layer**
   - higher-fidelity symbol extraction
   - richer relationships and language-native understanding
   - confidence-aware merge over syntax output

Long-term expectation:

- file layer is universal for recognized files
- syntax layer is the normal baseline for recognized code languages
- semantic layer is additive enrichment where it materially improves results
- file-only indexing remains as the rare fallback when syntax extraction truly
  does not exist or is intentionally deferred

## Explicit Project Decisions

### 1. Optimize for long-term architecture, not backward compatibility

This project is still early enough that preserving awkward interfaces,
intermediate schemas, or transitional crate boundaries is not a goal by
default.

If a cleaner long-term architecture requires:

- schema changes
- query contract adjustments
- crate refactors
- adapter/routing model redesign
- metric renaming

then those changes should be made deliberately now rather than deferred.

Backward compatibility should be treated as a conscious choice only where it
serves clear user value, not as the default constraint for this initiative.

### 2. Syntax indexing becomes the platform baseline

CodeAtlas should stop treating syntax extraction as a sparse, language-by-
language exception. The system should instead define syntax indexing as the
canonical baseline for recognized code languages.

This likely implies a central syntax indexing subsystem rather than a growing
set of loosely-related one-off adapters.

### 3. Semantic indexing is enrichment, not the primary baseline

Semantic adapters should still exist and matter, but they should enrich a
syntax-derived baseline rather than being the only path to useful symbol
coverage in many ecosystems.

### 4. File-only indexing remains necessary but should become rare

File-only indexing still matters for:

- unsupported or intentionally deferred languages
- malformed source that cannot be parsed syntactically
- temporary parser/extractor failures
- non-code assets that are still useful to retrieve by file

But it should not remain the normal outcome for high-value languages such as
PHP, Python, Go, Java, JavaScript, or Ruby.

### 5. Architecture clarity is more important than preserving current seams

The current `LanguageAdapter` abstraction should not remain the long-term
center of the indexing architecture.

Decision:

- replace the current unified adapter abstraction with an explicit split between
  syntax backends, semantic backends, and merge policy
- replace the current unified routing model with an explicit registry and
  dispatch-planning model
- treat this refactor as foundational work for the epic, not as optional
  cleanup

Rationale:

- syntax indexing is becoming the platform baseline rather than a sparse
  special case
- semantic runtime lifecycle concerns do not belong in the same abstraction as
  broad syntax extraction
- merge behavior should be a first-class platform concern rather than an
  incidental consequence of adapter shape

### 6. Correctness beats incremental scope

Scope for this initiative should be determined by what produces the correct
long-term architecture, not by what appears minimally invasive.

Decision:

- if the correct architecture requires a broader first slice, take the broader
  first slice
- if the correct architecture requires deeper schema or crate refactors, make
  those refactors now
- do not optimize the epic around being "small" if that leaves known
  architectural debt in place

## Recommended Architecture Direction

### Capability model

The platform represents indexing capability via explicit tiers. The ratified
tier model is defined in the architecture doc. The durable steady-state tiers
are:

- `FileOnly`
- `SyntaxOnly`
- `SyntaxPlusSemantic`

`SemanticOnly` exists only as a transitional migration state for languages that
have semantic backends but no syntax backend yet (TypeScript, Kotlin during
Phase 3). It is not a target product shape and should be resolved by adding
syntax backends for those languages.

This classification drives:

- metrics
- diagnostics
- quality reporting
- query responses where useful
- benchmark interpretation

### Syntax subsystem

Introduce a dedicated multi-language syntax indexing subsystem built around
tree-sitter as the primary syntax backend.

Recommended responsibilities:

- grammar registration / lifecycle
- parser management
- language-specific extraction modules
- shared AST/query utilities
- shared span conversion
- shared symbol ID inputs
- shared normalization of symbol kinds

This should be treated as platform infrastructure, not just a Rust-specific
adapter extended indefinitely.

### Language extraction modules

Within the syntax subsystem, add language-specific extraction modules for
high-value ecosystems. The first wave (Epic 17, Tickets 4-8) covers:

- PHP
- Python
- Go
- Java
- JavaScript

Later waves can expand into:

- Ruby
- C / C++
- C#
- Swift
- Shell
- SQL
- additional ecosystem-specific structured formats where symbol-level indexing
  is useful

Each module should define:

- supported symbol kinds
- parent/child relationships
- namespace/module behavior
- naming normalization
- known limitations

### Semantic subsystem

Semantic backends should remain separate from syntax extraction and should
produce enrichment signals that merge into the same canonical model:

- better symbol fidelity
- relationships
- language-native semantics
- confidence wins over syntax where justified

The merge layer should remain deterministic and explainable.

Recommended top-level subsystem split:

- `SyntaxBackend`
- `SemanticBackend`
- `MergeEngine`
- `CapabilityClassifier`
- `BackendRegistry`
- `DispatchPlanner`

These names are descriptive rather than final API commitments, but the
architectural split itself is intentional and should drive the refactor.

Capability-tier enum (ratified):

- `FileOnly`
- `SyntaxOnly`
- `SyntaxPlusSemantic`
- `SemanticOnly` — transitional only, not a durable product tier

`SemanticOnly` exists to accurately classify TypeScript/Kotlin during Phase 3
migration. It should be resolved by adding syntax backends for those languages,
at which point the variant can be removed.

### Core model evolution

The canonical model should be reconsidered now, before broad syntax rollout
locks in a schema that is too shallow for multi-language indexing.

Decision:

- perform the core-model refactor required for the long-term architecture in
  the first slice of the epic
- do not defer schema/model corrections purely to keep the initial
  implementation smaller

Areas likely worth promoting to first-class fields:

- container / parent relationships
- namespace or module path
- raw language-native symbol kind
- visibility and other common modifiers where available
- capability/provenance tier
- stable byte and line ranges as canonical retrieval primitives

The model/metrics design should also preserve diagnostic distinction between:

- file-only because a language or backend is unsupported
- file-only because execution was intentionally disabled by policy
- file-only because a backend failed at runtime

The objective is not to expose every field immediately, but to avoid a schema
that forces repeated incompatible migrations as language coverage expands.

### Query model evolution

The query layer should assume broad syntax availability over time and optimize
for:

- exact symbol lookup
- file outline as a baseline query surface across many languages
- symbol search across most recognized code languages
- precise source-slice retrieval
- clean behavior when only file or syntax capability exists

## Scope

### In scope

- define the canonical long-term indexing architecture
- make syntax indexing the platform baseline for recognized code languages
- refactor current adapter/routing assumptions where they obstruct that goal
- evolve core model, metrics, and query expectations as needed
- implement the first production-grade syntax fallback wave on the new platform
- update docs and benchmarks to measure file, syntax, and semantic coverage

### Out of scope

- preserving current interfaces purely for compatibility
- hosted deployment concerns unrelated to indexing architecture
- domain/framework enrichment beyond what is needed to prove the new baseline
- broad semantic parity across all languages in the first execution slice

## Initial proving ground

The first serious validation target should be a large Laravel/PHP repository.

Why:

- PHP is currently recognized but file-only in CodeAtlas
- Laravel is a common structure where syntax indexing materially helps AI
  workflows
- parity against current file-only behavior will be easy to observe

Expected outcome for the proving ground:

- non-zero PHP symbol coverage
- useful file outlines for controllers, models, services, commands, jobs, and
  tests
- meaningful token/context reduction compared with file-only retrieval

Additional validation targets:

### Validation target 2: CodeAtlas itself

Why:

- mixed-language repository
- useful self-hosting benchmark
- good validation for file/syntax/semantic coexistence in one repo

Expected outcome:

- no regression in existing Rust, TypeScript, and Kotlin behavior
- improved syntax coverage for other recognized code/config languages where
  supported
- query surfaces remain coherent on a mixed-capability repo

### Validation target 3: Android/Kotlin repository

Why:

- validates semantic-over-syntax layering on a language where semantic support
  already exists
- proves that the new syntax platform does not weaken the semantic path

Expected outcome:

- Kotlin semantic behavior remains strong
- syntax baseline and semantic enrichment coexist cleanly
- merge/provenance semantics remain understandable and deterministic

## Epic shape

This should be planned as one architecture epic with multiple child tickets.
The epic is intentionally larger than prior slices because it establishes the
long-term indexing platform rather than a narrow feature increment.

Ticket sequencing note:

- Ticket 1 is intentionally architecture-spec only, but it must not be vague
  or purely aspirational
- Ticket 1 should produce the actual replacement interface and crate-boundary
  design in enough detail that later tickets are implementing an agreed design,
  not reopening architectural indecision
- production crate restructuring starts after Ticket 1, not during it

## Proposed issue breakdown

### Epic 17: Universal Syntax Indexing Platform

Objective:

Make syntax indexing the default baseline for recognized code languages in
CodeAtlas, with semantic indexing layered on top and file-only indexing
reserved as the explicit last fallback.

### Ticket 1: Define the universal syntax indexing architecture

Scope:

- define capability tiers (`file`, `syntax`, `semantic`)
- define subsystem boundaries for syntax backends, semantic backends, and merge
- replace the current unified adapter/routing abstraction with explicit syntax,
  semantic, merge, registry, and dispatch roles
- document compatibility stance and migration expectations

Deliverables:

- canonical architecture doc updates
- actual proposed Rust-facing replacement interfaces
- actual crate-boundary and data-flow decisions
- explicit migration plan for existing indexing components
- explicit replacement design for `AdapterRouter` / `AdapterPolicy`

Non-goals:

- no production crate restructuring in this ticket
- no partial implementation disguised as architecture work

### Ticket 2: Refactor core model and metrics for capability tiers

Scope:

- evolve canonical schemas where needed
- make capability/provenance explicit
- update metrics and reporting to distinguish file, syntax, and semantic
  coverage

### Ticket 3: Create the multi-language syntax indexing subsystem

Scope:

- add grammar registry / parser lifecycle
- add shared extraction utilities
- establish the language-module pattern for syntax extractors
- migrate the current Rust syntax path onto the new subsystem so the platform
  is exercised by at least one in-tree production language immediately

Rationale:

- the platform should not exist only as abstract infrastructure
- Rust is the current syntax-bearing baseline and should be folded into the new
  syntax subsystem rather than left behind on the retired abstraction

### Ticket 4: Implement PHP syntax indexing on the new subsystem

Scope:

- production-grade PHP extraction
- Laravel-oriented acceptance coverage
- file outline / search usability for PHP codebases

### Ticket 5: Implement Python syntax indexing

Scope:

- production-grade Python extraction
- regression coverage for Python

### Ticket 6: Implement Go syntax indexing

Scope:

- production-grade Go extraction
- regression coverage for Go

### Ticket 7: Implement Java syntax indexing

Scope:

- production-grade Java extraction
- regression coverage for Java

### Ticket 8: Implement JavaScript syntax indexing

Scope:

- production-grade JavaScript extraction
- regression coverage for JavaScript

### Ticket 9: Rework query surfaces for broad syntax coverage

Scope:

- ensure symbol search and file outline semantics remain clean across capability
  tiers
- add or improve exact slice retrieval if needed for syntax-first workflows

Expected acceptance shape:

- capability-tier behavior must be defined in a way that can be asserted in
  integration tests rather than described only qualitatively

### Ticket 10: Update benchmark and token-efficiency evaluation strategy

Scope:

- measure file, syntax, and semantic coverage separately
- add comparison guidance for file-only versus syntax-indexed repos
- validate token/context reduction on PHP/Laravel and other newly supported
  ecosystems

## Definition of done for the epic

- syntax indexing is the normal baseline for major recognized code languages
- PHP/Laravel is no longer file-only
- file-only indexing is preserved but is no longer the expected outcome for
  most common code repositories
- metrics, docs, and benchmarks reflect the layered capability model clearly
- the architecture is positioned for continued language expansion without
  repeated foundational redesign

# Universal Syntax Indexing Architecture

Status: Draft — canonical technical design target for Epic 17

This document defines the target architecture for moving CodeAtlas from a
narrow adapter set plus file-level fallback into a platform where syntax
indexing is the default baseline for major recognized code languages and
semantic indexing is an enrichment layer on top of that baseline.

It is the technical companion to the planning artifact in
`docs/planning/universal-syntax-indexing.md`.

## Purpose

CodeAtlas needs a durable architecture that supports:

- broad multi-language syntax indexing
- semantic enrichment without conflating it with syntax extraction
- clean capability-tier reporting (`file`, `syntax`, `semantic`)
- continued language expansion without repeated foundational redesign

This document exists so implementation tickets can target one technical design
rather than re-deciding architecture inside ticket bodies.

## Canonical Product Shape

The product should evolve toward three explicit indexing capability layers:

1. **File layer**
   - file record
   - content blob
   - file tree / repo outline visibility
   - exact file-content retrieval

2. **Syntax layer**
   - broad multi-language symbol extraction
   - stable spans and symbol IDs
   - file outline and symbol search across most recognized code languages

3. **Semantic layer**
   - higher-fidelity symbols
   - language-native relationships and enrichment
   - deterministic merge over syntax output

File-only indexing remains valid, but it should no longer be the expected
steady-state for common code languages.

## Architectural Decisions

### AD-1: Retire `LanguageAdapter` as the central long-term abstraction

**Decision:** The current unified `LanguageAdapter` abstraction should not
remain the center of the indexing architecture.

**Replacement direction:** Introduce explicit roles for:

- `SyntaxBackend`
- `SemanticBackend`
- `MergeEngine`
- `CapabilityClassifier`

**Rationale:**

- syntax indexing is becoming the platform baseline rather than a sparse
  special case
- semantic runtime lifecycle concerns do not belong in the same abstraction as
  broad syntax extraction
- merge policy should be an explicit platform concern

### AD-2: Build a dedicated syntax subsystem

**Decision:** Create a dedicated multi-language syntax subsystem backed by
tree-sitter.

**Rationale:**

- tree-sitter gives broad language coverage, stable spans, and deterministic
  extraction
- broad syntax support is platform infrastructure, not an incidental adapter
  implementation detail

### AD-3: Perform foundational model refactors in the first slice

**Decision:** If the core model or crate boundaries are insufficient for the
long-term architecture, refactor them in the first slice of the epic.

**Rationale:** Deferring known model corrections would create avoidable
architecture debt while the project is still early.

### AD-4: Rust migrates onto the new syntax subsystem

**Decision:** The existing Rust syntax path should be migrated onto the new
syntax subsystem rather than preserved as a legacy special case.

**Rationale:** The new subsystem should be validated immediately by an existing
production language and should not coexist indefinitely with a retired
architecture.

## Target Crate/Subsystem Structure

This is the recommended target shape. The exact crate names may change, but the
responsibility split should remain close to this.

```
crates/
  core-model/                  Canonical schemas and stable IDs
  repo-walker/                 Discovery and language detection
  syntax-platform/             Tree-sitter grammar registry, parser lifecycle,
                               shared extraction utilities, language modules
  semantic-api/                Semantic backend trait(s) and shared semantics
  semantic-typescript/         TypeScript semantic backend
  semantic-kotlin/             Kotlin semantic backend
  indexer/                     Orchestration: discovery -> syntax -> semantic ->
                               merge -> enrich -> persist
  store/                       MetadataStore + BlobStore
  query-engine/                Retrieval and structure queries
  service/                     Persistent local HTTP service
  server-mcp/                  MCP tool registry and contracts
  cli/                         Local command surface
```

### Notes

- `adapter-api` likely disappears or is reduced dramatically.
- `adapter-syntax-treesitter` should not remain the long-term center of syntax
  work; its useful parts should migrate into `syntax-platform`.
- semantic backends should depend on a smaller semantic-facing contract rather
  than the current broad language-adapter abstraction.

## Rust-Facing Interface Direction

These are architectural interface targets, not final code signatures.

### Syntax backend layer

```rust
pub trait SyntaxBackend {
    fn language(&self) -> &'static str;
    fn capability(&self) -> SyntaxCapability;
    fn extract_symbols(
        &self,
        file: &PreparedFile,
    ) -> Result<SyntaxExtraction, SyntaxError>;
}
```

Key properties:

- no semantic runtime lifecycle concerns
- deterministic extraction from prepared file content
- one backend per language module or a registry-backed dispatcher

### Semantic backend layer

```rust
pub trait SemanticBackend {
    fn language(&self) -> &'static str;
    fn capability(&self) -> SemanticCapability;
    fn enrich_symbols(
        &self,
        file: &PreparedFile,
        syntax: Option<&SyntaxExtraction>,
    ) -> Result<SemanticExtraction, SemanticError>;
}
```

Key properties:

- semantic backends can optionally consume syntax output
- lifecycle/process/runtime management remains confined here
- semantic output is enrichment, not the only symbol source

### Merge layer

```rust
pub trait MergeEngine {
    fn merge(
        &self,
        syntax: Option<SyntaxExtraction>,
        semantic: Option<SemanticExtraction>,
    ) -> MergeResult;
}
```

Key properties:

- deterministic precedence rules
- explicit provenance
- no backend-specific policy hidden in extractors

### Capability classifier

```rust
pub trait CapabilityClassifier {
    fn classify(result: &MergeResult) -> CapabilityTier;
}
```

Capability tiers:

```rust
pub enum CapabilityTier {
    FileOnly,
    SyntaxOnly,
    SemanticOnly,
    SyntaxPlusSemantic,
}
```

## Pipeline Direction

The indexing pipeline should evolve toward:

1. **Discover**
   - walk files
   - detect language
   - load content

2. **Persist file baseline**
   - file record + blob remain universal for recognized files

3. **Syntax extraction**
   - run syntax backend when available

4. **Semantic extraction**
   - run semantic backend when available

5. **Merge**
   - canonical symbol set
   - provenance + capability classification

6. **Enrich**
   - summaries / keywords / metadata enrichment where still useful

7. **Persist symbols + aggregates**
   - write canonical output
   - update repo/file/symbol aggregates

Important consequence:

- the pipeline should stop thinking of “parse stage” as a single opaque adapter
  action
- syntax, semantic, and merge become visible stages or sub-stages

## Core Model Direction

The current model is likely too shallow for broad syntax indexing if left
unchanged. The first slice of Epic 17 should decide and implement the model
refactor needed for the long-term architecture.

Fields likely worth making first-class:

- `capability_tier`
- `source_backend_kind` or equivalent provenance metadata
- `raw_language_symbol_kind`
- `container_symbol_id` / parent relationship
- `namespace_path`
- stable byte range and line range as canonical retrieval fields
- common modifiers where available (`visibility`, `static`, `abstract`, etc.)

The exact shape can vary, but the decision should optimize for:

- broad multi-language syntax extraction
- explainable semantic-over-syntax merge
- future relationship indexing

## Query Layer Direction

The query layer should be built around capability tiers rather than sparse
symbol support.

Expected behavior:

- file-only repos:
  - file tree works
  - repo outline works
  - file outline returns file metadata plus zero symbols
  - file-content retrieval works

- syntax-indexed repos:
  - symbol search works
  - exact symbol lookup works
  - file outline returns syntax-derived symbols

- syntax-plus-semantic repos:
  - symbol search and exact lookup return merged canonical results
  - provenance and capability semantics remain deterministic

Potential follow-on:

- precise source-slice retrieval by canonical byte/line range

## Migration Direction

### Existing Rust syntax path

- migrate Rust into the new syntax subsystem in the platform ticket
- do not leave Rust on the retired abstraction

### Existing semantic backends

- TypeScript and Kotlin should be adapted to the new semantic backend contract
- semantic output should merge with syntax output using the new merge layer

### Existing metrics

- move from sparse symbol-era semantics toward explicit file/syntax/semantic
  coverage reporting

### Existing crate boundaries

- if `adapter-api` becomes an awkward compatibility shell, replace or remove it
  rather than preserving it for familiarity

## Validation Strategy

Three validation targets should be used before declaring the architecture ready:

### 1. Laravel/PHP repository

Purpose:

- prove that a formerly file-only ecosystem now gains useful syntax indexing

Expected evidence:

- PHP symbols extracted
- useful outlines/search on controllers, models, services, commands, jobs, tests
- better token/context behavior than file-only retrieval

### 2. CodeAtlas repository

Purpose:

- validate mixed-language coexistence on the project’s own codebase

Expected evidence:

- no regression in Rust, TypeScript, or Kotlin behavior
- coherent behavior for mixed file/syntax/semantic capability tiers

### 3. Android/Kotlin repository

Purpose:

- validate semantic-over-syntax layering on an existing semantic-supported
  ecosystem

Expected evidence:

- Kotlin semantic behavior remains strong
- merge/provenance remains deterministic and understandable

## Definition Of Ready For Implementation

Implementation should not begin until this architecture is explicit enough that
Ticket 1 can be reviewed as a concrete technical design, not a second planning
pass.

Minimum readiness:

- subsystem responsibilities are explicit
- replacement Rust-facing interfaces are explicit enough to code against
- core-model refactor direction is explicit
- migration path for Rust and existing semantic backends is explicit
- validation targets and success criteria are explicit

## Relationship To Planning Docs

- product/epic framing lives in:
  `docs/planning/universal-syntax-indexing.md`
- issue decomposition lives in:
  `docs/planning/github-issues/universal-syntax-*.md`
- this document is the canonical technical design source for Ticket 1 and all
  later implementation tickets in Epic 17

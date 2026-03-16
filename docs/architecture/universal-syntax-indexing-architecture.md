# Universal Syntax Indexing Architecture

Status: Ratified — canonical technical design for Epic 17 (Ticket 1 gate)

This document defines the architecture for moving CodeAtlas from a narrow
adapter set plus file-level fallback into a platform where syntax indexing is
the default baseline for major recognized code languages and semantic indexing
is an enrichment layer on top of that baseline.

It is the technical companion to the planning artifact in
`docs/planning/universal-syntax-indexing.md`.

All implementation tickets in Epic 17 (Tickets 2-10) implement the design
described here. Architectural questions that arise during implementation should
be resolved by updating this document first, not by ad-hoc decisions in ticket
bodies.

## Purpose

CodeAtlas needs a durable architecture that supports:

- broad multi-language syntax indexing
- semantic enrichment without conflating it with syntax extraction
- clean capability-tier reporting (`file`, `syntax`, `semantic`)
- continued language expansion without repeated foundational redesign

## Compatibility Stance

Clean long-term architecture is favored over backward compatibility for this
initiative. This is consistent with the project's early-stage architecture
policy (`docs/engineering-principles.md`).

Concretely:

- the current `LanguageAdapter` / `AdapterRouter` / `AdapterPolicy`
  abstractions are retired and replaced, not wrapped
- schema migrations, crate restructuring, and metric renaming are permitted
  when they serve the target architecture
- no compatibility shim is required unless it makes implementation materially
  cleaner during the transition
- backward compatibility should be a conscious product decision where it serves
  clear user value, not the default constraint

## Capability Model

The platform defines three explicit indexing capability tiers:

### Tier definitions

| Tier                   | Meaning                                                   | Durable? |
|------------------------|-----------------------------------------------------------|----------|
| `FileOnly`             | File record and content blob only; no extracted symbols   | yes      |
| `SyntaxOnly`           | Symbols extracted by a syntax backend (tree-sitter)       | yes      |
| `SyntaxPlusSemantic`   | Syntax baseline enriched by a semantic backend            | yes      |
| `SemanticOnly`         | Semantic backend produced symbols; no syntax backend ran  | **no** — transitional only |

### Tier roles

- **File layer** is universal for all recognized files: file record, content
  blob, file tree / repo outline visibility, exact file-content retrieval.
- **Syntax layer** is the platform baseline for recognized code languages:
  broad multi-language symbol extraction, stable spans and symbol IDs, file
  outline and symbol search.
- **Semantic layer** is enrichment over a syntax baseline: higher-fidelity
  symbols, language-native relationships, deterministic merge over syntax
  output.

### `SemanticOnly` is not a durable tier

`SemanticOnly` exists as a variant in `CapabilityTier` but is explicitly
transitional, not a durable product tier. In the long-term architecture,
semantic indexing is enrichment layered on top of a syntax baseline.

During migration (specifically for TypeScript and Kotlin, which currently have
semantic backends but no syntax backends on the new subsystem), the system will
execute semantic-only paths. These are classified as `CapabilityTier::SemanticOnly`
so that metrics and diagnostics accurately report their state rather than
misclassifying them as `FileOnly`.

The `SemanticOnly` tier should be resolved by adding syntax backends for the
affected languages. Once no files are classified as `SemanticOnly`, the variant
can be removed. The migration plan (see Migration section) defines how these
transitional states are resolved.

### Diagnostic distinctions

The capability model preserves diagnostic distinctions between:

- file-only because a language is unrecognized → `FileOnlyReason::LanguageUnrecognized`
- file-only because no syntax backend is registered → `FileOnlyReason::NoSyntaxBackendRegistered`
- file-only because syntax was disabled by policy → `FileOnlyReason::SyntaxDisabledByPolicy`
- file-only because all syntax backends failed at runtime → `FileOnlyReason::AllSyntaxBackendsFailed`
- syntax-only because no semantic backend is registered → normal `SyntaxOnly` outcome
- syntax-only because semantic backend failed → visible in `ExecutionOutcome.semantic_attempts`

These distinctions are surfaced in structured logging, metrics, and diagnostic
queries. They are not collapsed into a single "no symbols" state.

## Architectural Decisions

### AD-1: Retire `LanguageAdapter` as the central abstraction

**Decision:** The current unified `LanguageAdapter` trait, `AdapterRouter`
trait, and `AdapterPolicy` enum are retired. They are replaced by the explicit
subsystem roles defined below.

**What is retired:**

- `LanguageAdapter` trait (`adapter-api/src/lib.rs`)
- `AdapterRouter` trait (`adapter-api/src/lib.rs`)
- `AdapterPolicy` enum (`adapter-api/src/lib.rs`)
- `DefaultRouter` (`adapter-api/src/router.rs`)
- `AdapterCapabilities` struct (`adapter-api/src/lib.rs`)
- `AdapterOutput` struct (`adapter-api/src/lib.rs`)
- `default_policy()` function (`adapter-api/src/router.rs`)

**Replacement:** The explicit subsystem roles `SyntaxBackend`,
`SemanticBackend`, `MergeEngine`, `CapabilityClassifier`, `BackendRegistry`,
and `DispatchPlanner` defined in the Interface Contracts section below.

**Rationale:**

- syntax indexing is becoming the platform baseline, not a sparse special case
- semantic runtime lifecycle concerns (process management, runtime startup) do
  not belong in the same abstraction as deterministic syntax extraction
- merge policy should be an explicit platform concern, not an incidental
  consequence of adapter shape
- the old `SemanticPreferred` / `SyntaxOnly` policy model assumes one unified
  adapter selection path; the new architecture has separate syntax and semantic
  subsystems that participate independently

### AD-2: Build a dedicated syntax subsystem

**Decision:** Create a dedicated multi-language syntax subsystem backed by
tree-sitter, housed in the `syntax-platform` crate.

**Rationale:**

- tree-sitter provides broad language coverage, stable spans, and deterministic
  extraction
- broad syntax support is platform infrastructure, not an incidental adapter
  implementation detail
- a single subsystem avoids the current pattern of loosely-related one-off
  adapters

### AD-3: Perform foundational model refactors in the first implementation slice

**Decision:** Core model and crate boundary changes required for the long-term
architecture are made in Ticket 2, before broad syntax rollout locks in a
schema that is too shallow for multi-language indexing.

**Rationale:** Deferring known model corrections would create avoidable
architecture debt while the project is still early.

### AD-4: Rust migrates onto the new syntax subsystem

**Decision:** The existing Rust syntax path (currently in
`adapter-syntax-treesitter`) migrates onto the new `syntax-platform` subsystem
in Ticket 3. It is not preserved as a legacy special case.

**Rationale:** The new subsystem should be validated immediately by an existing
production language and should not coexist indefinitely with a retired
architecture.

### AD-5: Registry supports multiple backends per tier per language

**Decision:** The `BackendRegistry` returns `Vec<BackendId>` for both syntax
and semantic lookups. The architecture does not assume exactly one backend of
each kind per language.

**Rationale:** While the initial implementation will have one syntax backend per
language, the registry design should not create an artificial ceiling that
requires redesign when multi-backend scenarios arise (e.g. experimental vs
stable syntax grammars, or multiple semantic providers for the same language).

## Target Crate Structure

### Crate layout

```
crates/
  core-model/                  Canonical schemas, stable IDs, CapabilityTier
  repo-walker/                 Discovery and language detection
  syntax-platform/             Tree-sitter grammar registry, parser lifecycle,
                               shared extraction utilities, language modules
  semantic-api/                SemanticBackend trait, shared semantic types
  semantic-typescript/         TypeScript semantic backend
  semantic-kotlin/             Kotlin semantic backend
  indexer/                     Orchestration: discovery → dispatch → syntax →
                               semantic → merge → enrich → persist
  store/                       MetadataStore + BlobStore
  query-engine/                Retrieval and structure queries
  service/                     Persistent local HTTP service
  server-mcp/                  MCP tool registry and contracts
  cli/                         Local command surface
```

### Crate dependency graph

```
cli ──→ service ──→ server-mcp ──→ query-engine ──→ store
                                                      ↑
                    indexer ───────────────────────────┘
                      ↑
         ┌────────────┼────────────────┐
         ↓            ↓                ↓
  syntax-platform  semantic-api    core-model
                      ↑               ↑
              ┌───────┴──────┐        │
              ↓              ↓        │
  semantic-typescript  semantic-kotlin │
                                      │
  repo-walker ────────────────────────┘
```

Key dependency rules:

- `syntax-platform` depends on `core-model` only (no dependency on
  `semantic-api`, `indexer`, or `store`)
- `semantic-api` depends on `core-model` and `syntax-platform` (semantic
  backends receive `SyntaxExtraction` as input)
- `indexer` depends on `syntax-platform`, `semantic-api`, `core-model`,
  `repo-walker`, and `store`
- `indexer` owns the `BackendRegistry`, `DispatchPlanner`, `MergeEngine`, and
  `CapabilityClassifier` implementations
- semantic backend crates (`semantic-typescript`, `semantic-kotlin`) depend on
  `semantic-api` and `core-model`, not on `indexer`

### Crate retirement

- `adapter-api` is removed entirely. Its useful type definitions
  (`SourceSpan`, `ExtractedSymbol` equivalents) move into `core-model` or
  `syntax-platform` as appropriate.
- `adapter-syntax-treesitter` is removed. Its grammar registry, language
  profiles, and extraction logic move into `syntax-platform`.

## Interface Contracts

These are the concrete trait and type definitions that implementation tickets
code against. They supersede the retired `LanguageAdapter` / `AdapterRouter` /
`AdapterPolicy` abstractions.

### Types shared across subsystems

These types live in `core-model`:

```rust
/// Capability tier for a file after indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    FileOnly,
    SyntaxOnly,
    SyntaxPlusSemantic,
    /// Transitional state: semantic backend produced symbols but no syntax
    /// backend was available. This tier exists only to support the Phase 3
    /// migration for TypeScript and Kotlin. It is NOT a durable product
    /// tier and should be resolved by adding syntax backends for the
    /// affected languages.
    SemanticOnly,
}

/// Why a file was indexed as file-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOnlyReason {
    /// Language was not recognized by the walker.
    LanguageUnrecognized,
    /// No syntax backend registered for this language.
    NoSyntaxBackendRegistered,
    /// Syntax extraction was disabled by policy for this language.
    SyntaxDisabledByPolicy,
    /// All registered syntax backends failed at runtime.
    AllSyntaxBackendsFailed,
}

/// Opaque backend identifier. Format: "{kind}-{language}" (e.g.
/// "syntax-rust", "semantic-typescript").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackendId(pub String);
```

### Syntax backend (`syntax-platform`)

```rust
/// Describes the symbol kinds and features a syntax backend supports.
#[derive(Debug, Clone)]
pub struct SyntaxCapability {
    /// Symbol kinds this backend can extract.
    pub supported_kinds: Vec<SymbolKind>,
    /// Whether this backend extracts parent/container relationships.
    pub supports_containers: bool,
    /// Whether this backend extracts doc comments.
    pub supports_docs: bool,
}

/// A symbol extracted by a syntax backend.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
}

/// Result of syntax extraction for a single file.
#[derive(Debug, Clone)]
pub struct SyntaxExtraction {
    pub language: String,
    pub symbols: Vec<SyntaxSymbol>,
    pub backend_id: BackendId,
}

/// Errors produced by syntax extraction.
#[derive(Debug, thiserror::Error)]
pub enum SyntaxError {
    #[error("parse failed for {path}: {reason}")]
    Parse { path: PathBuf, reason: String },

    #[error("unsupported language: {language}")]
    Unsupported { language: String },
}

/// Contract for a syntax extraction backend.
///
/// Implementations are deterministic: the same file content always produces
/// the same extraction result. No external process or runtime is required.
pub trait SyntaxBackend: Send + Sync {
    /// The language this backend handles.
    fn language(&self) -> &str;

    /// Describes what this backend can extract.
    fn capability(&self) -> &SyntaxCapability;

    /// Extract symbols from a prepared file.
    ///
    /// The file's language must match `self.language()`. Returns
    /// `SyntaxError::Unsupported` if it does not.
    fn extract_symbols(
        &self,
        file: &PreparedFile,
    ) -> Result<SyntaxExtraction, SyntaxError>;
}
```

`PreparedFile` is the existing type from `indexer/src/stage.rs` (discovery
stage output), promoted to a shared definition so that both syntax and semantic
backends can receive it without depending on the indexer crate:

```rust
/// A file ready for backend processing. Produced by the discovery stage.
/// Lives in `core-model` or `syntax-platform` (TBD by Ticket 2).
#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub language: String,
    pub content: Vec<u8>,
}
```

### Syntax merge baseline

When multiple syntax backends contribute extractions for the same file, their
results are merged into a `SyntaxMergeBaseline` before semantic backends run.
This type lives in `syntax-platform` (or `core-model` — TBD by Ticket 2):

```rust
/// Merged result of all syntax backends for a single file.
///
/// This is the canonical syntax-derived symbol set that semantic backends
/// receive as input. It is produced by the merge engine's syntax-merge
/// phase and represents one consistent view of the file's symbols, even
/// when multiple syntax backends contributed.
#[derive(Debug, Clone)]
pub struct SyntaxMergeBaseline {
    pub language: String,
    pub symbols: Vec<SyntaxSymbol>,
    /// Backend IDs that contributed to this baseline.
    pub contributing_backends: Vec<BackendId>,
}
```

When only one syntax backend runs (the common case in Epic 17), the merge
engine produces the `SyntaxMergeBaseline` trivially from that single
extraction. The type exists so the contract is correct when multi-backend
scenarios arise later without requiring a signature change on
`SemanticBackend::enrich_symbols`.

### Semantic backend (`semantic-api`)

```rust
/// Describes the enrichment features a semantic backend provides.
#[derive(Debug, Clone)]
pub struct SemanticCapability {
    /// Whether this backend can resolve type references.
    pub supports_type_refs: bool,
    /// Whether this backend can resolve call-site references.
    pub supports_call_refs: bool,
    /// Default confidence score for symbols produced by this backend.
    pub default_confidence: f32,
}

/// A symbol produced by semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub confidence_score: Option<f32>,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
    /// Type references resolved by semantic analysis.
    pub type_refs: Vec<String>,
    /// Call-site references resolved by semantic analysis.
    pub call_refs: Vec<String>,
}

/// Result of semantic enrichment for a single file.
#[derive(Debug, Clone)]
pub struct SemanticExtraction {
    pub language: String,
    pub symbols: Vec<SemanticSymbol>,
    pub backend_id: BackendId,
    pub default_confidence: f32,
}

/// Errors produced by semantic extraction.
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    #[error("semantic analysis failed for {path}: {reason}")]
    Analysis { path: PathBuf, reason: String },

    #[error("semantic runtime unavailable: {reason}")]
    RuntimeUnavailable { reason: String },

    #[error("unsupported language: {language}")]
    Unsupported { language: String },
}

/// Contract for a semantic enrichment backend.
///
/// Unlike syntax backends, semantic backends may require external runtimes
/// (e.g. a TypeScript language server process). Lifecycle management for
/// these runtimes is the backend's responsibility.
///
/// Semantic backends receive the merged syntax baseline (not individual
/// per-backend extractions) so they can enrich rather than duplicate
/// syntax extraction work. See the Multi-Backend Merge Design section
/// for the full rationale.
pub trait SemanticBackend: Send + Sync {
    /// The language this backend handles.
    fn language(&self) -> &str;

    /// Describes what this backend can produce.
    fn capability(&self) -> &SemanticCapability;

    /// Enrich or produce symbols for a file.
    ///
    /// `syntax_baseline` is the merged result of all syntax backends for
    /// this file. It is `None` when no syntax backend was available or all
    /// syntax backends failed (transitional semantic-only path).
    ///
    /// Semantic backends may use the baseline as context for enrichment or
    /// ignore it entirely (e.g. when the semantic backend already has its
    /// own full extraction capability).
    fn enrich_symbols(
        &self,
        file: &PreparedFile,
        syntax_baseline: Option<&SyntaxMergeBaseline>,
    ) -> Result<SemanticExtraction, SemanticError>;
}
```

### Merge engine (`indexer`)

```rust
/// Provenance metadata for a merged symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedSymbolProvenance {
    pub backend_id: BackendId,
    pub capability_tier: CapabilityTier,
    pub confidence_score: f32,
    pub merge_outcome: MergeOutcome,
}

/// Outcome of a merge decision for a single symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Only one source produced this symbol.
    Unique,
    /// Semantic version won over syntax version.
    SemanticWin,
    /// Syntax version won over semantic version (higher confidence).
    SyntaxWin,
    /// Equal confidence; semantic won by tiebreak rule.
    Tie,
    /// Multiple sources of the same tier; higher confidence won.
    SameTier,
}

/// Result of merging syntax and semantic extractions for a single file.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Canonical merged symbols for the file.
    pub symbols: Vec<MergedSymbol>,
    /// Per-symbol provenance, parallel to `symbols`.
    pub provenance: Vec<MergedSymbolProvenance>,
    /// The capability tier achieved for this file.
    pub capability_tier: CapabilityTier,
    /// Number of duplicate symbols resolved during merge.
    pub duplicates_resolved: usize,
}

/// A canonical merged symbol ready for persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub signature: String,
    pub confidence_score: f32,
    pub docstring: Option<String>,
    pub parent_qualified_name: Option<String>,
    pub type_refs: Vec<String>,
    pub call_refs: Vec<String>,
}

/// Merge engine contract. Lives in `indexer`.
///
/// The merge engine operates in two phases:
///
/// **Phase 1 — Syntax merge:** Merges multiple syntax extractions into a
/// single `SyntaxMergeBaseline`. This baseline is passed to semantic
/// backends as input. When only one syntax backend runs, this phase is
/// trivial (pass-through).
///
/// **Phase 2 — Final merge:** Merges the syntax baseline with semantic
/// extractions to produce the canonical `MergeResult`.
///
/// Merge rules (applied in both phases):
/// 1. Symbols are deduplicated by `(qualified_name, kind)`.
/// 2. Higher effective confidence wins.
/// 3. On confidence tie, semantic wins over syntax (phase 2 only).
/// 4. On same-tier tie, earlier backend (by registration order) wins.
/// 5. Non-overlapping symbols from both sources are all retained.
/// 6. Provenance is preserved for every symbol.
pub trait MergeEngine: Send + Sync {
    /// Phase 1: Merge multiple syntax extractions into a single baseline.
    ///
    /// Returns `None` if `extractions` is empty (no syntax output).
    fn merge_syntax(
        &self,
        extractions: &[SyntaxExtraction],
    ) -> Option<SyntaxMergeBaseline>;

    /// Phase 2: Merge the syntax baseline with semantic extractions to
    /// produce the final canonical symbol set.
    ///
    /// `syntax_baseline` is `None` when no syntax backend succeeded
    /// (transitional semantic-only path).
    fn merge_final(
        &self,
        syntax_baseline: Option<&SyntaxMergeBaseline>,
        semantic: &[SemanticExtraction],
    ) -> MergeResult;
}
```

### Multi-backend merge design rationale

The two-phase merge design resolves the question of what semantic backends
receive when multiple syntax backends exist for the same language:

**Decision:** Semantic backends always receive the merged syntax baseline
(`SyntaxMergeBaseline`), never individual per-backend extractions.

**Why not pass individual extractions?**

- Semantic enrichment is conceptually layered on top of the canonical syntax
  view of a file, not tied to one syntax backend's perspective
- Passing individual extractions would require running semantic enrichment once
  per syntax backend, multiplying cost
- It would create ambiguous provenance: which syntax-semantic combination
  produced the final canonical symbol?

**Why not defer merging until after semantic extraction?**

- Semantic backends that consume syntax output need a consistent baseline to
  enrich against
- A single-phase merge that receives raw extractions from both tiers would not
  know which syntax extraction a given semantic extraction was derived from,
  making provenance tracking unreliable

**Common case:** In Epic 17, each language has at most one syntax backend, so
`merge_syntax` is a trivial pass-through and the two-phase design has zero
overhead. The design exists so the contract is correct when multi-backend
scenarios arise later.

### Capability classifier (`indexer`)

```rust
/// Classifies the capability tier achieved for a file based on its
/// execution outcome.
///
/// Classification rules:
/// - If merge produced symbols from both syntax and semantic → SyntaxPlusSemantic
/// - If merge produced symbols from syntax only → SyntaxOnly
/// - If merge produced symbols from semantic only (no syntax backend ran
///   or all syntax backends failed) → SemanticOnly (transitional)
/// - Otherwise → FileOnly (with reason from the execution plan or attempts)
pub trait CapabilityClassifier: Send + Sync {
    fn classify(&self, outcome: &ExecutionOutcome) -> CapabilityTier;
}
```

### Registry and dispatch (`indexer`)

```rust
/// Registry of all available syntax and semantic backends.
///
/// Backends are registered at pipeline startup. The registry is immutable
/// after construction.
pub trait BackendRegistry: Send + Sync {
    /// Returns IDs of all syntax backends registered for a language.
    /// Returns empty vec if no syntax backend exists.
    fn syntax_backends(&self, language: &str) -> Vec<BackendId>;

    /// Returns IDs of all semantic backends registered for a language.
    /// Returns empty vec if no semantic backend exists.
    fn semantic_backends(&self, language: &str) -> Vec<BackendId>;

    /// Returns a reference to the syntax backend with the given ID.
    /// Panics if the ID is not registered (programming error).
    fn syntax(&self, id: &BackendId) -> &dyn SyntaxBackend;

    /// Returns a reference to the semantic backend with the given ID.
    /// Panics if the ID is not registered (programming error).
    fn semantic(&self, id: &BackendId) -> &dyn SemanticBackend;

    /// Returns all languages that have at least one registered syntax backend.
    fn syntax_languages(&self) -> Vec<&str>;

    /// Returns all languages that have at least one registered semantic backend.
    fn semantic_languages(&self) -> Vec<&str>;
}
```

```rust
/// Runtime context that influences dispatch decisions.
#[derive(Debug, Clone)]
pub struct DispatchContext {
    /// Controls whether syntax backends are invoked.
    pub syntax_policy: SyntaxPolicy,
    /// Controls whether semantic backends are invoked.
    pub semantic_policy: SemanticPolicy,
}

/// Policy governing syntax backend participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxPolicy {
    /// Invoke syntax backends when registered and available (normal default).
    EnabledWhenAvailable,
    /// Never invoke syntax backends. Files with this policy get
    /// FileOnly(SyntaxDisabledByPolicy) unless a semantic-only path applies.
    Disabled,
}

/// Policy governing semantic backend participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticPolicy {
    /// Never invoke semantic backends.
    Disabled,
    /// Invoke semantic backends when registered and available.
    EnabledWhenAvailable,
    /// Require semantic output; treat missing semantic as an error.
    Required,
}

/// Planned execution path for a single file.
#[derive(Debug, Clone)]
pub enum ExecutionPlan {
    /// No extraction will be attempted; file gets file-only indexing.
    FileOnly {
        reason: FileOnlyReason,
    },
    /// Extraction will be attempted with the listed backends.
    Execute {
        syntax: Vec<BackendId>,
        semantic: Vec<BackendId>,
    },
}

/// Full outcome of executing a plan for a single file.
#[derive(Debug)]
pub struct ExecutionOutcome {
    pub plan: ExecutionPlan,
    pub syntax_attempts: Vec<BackendAttempt<SyntaxExtraction, SyntaxError>>,
    pub semantic_attempts: Vec<BackendAttempt<SemanticExtraction, SemanticError>>,
    pub merge_result: Option<MergeResult>,
}

/// Result of a single backend invocation.
#[derive(Debug)]
pub struct BackendAttempt<T, E> {
    pub backend: BackendId,
    pub result: Result<T, E>,
}

/// Dispatch planner contract.
///
/// Given a file's language, the registry contents, and runtime context,
/// produces an execution plan.
///
/// Planning rules:
/// 1. If syntax_policy is Disabled → FileOnly(SyntaxDisabledByPolicy)
///    (unless semantic backends exist and semantic_policy permits, in which
///    case proceed to rule 4 with an empty syntax list)
/// 2. If no syntax backend is registered → FileOnly(NoSyntaxBackendRegistered)
///    (unless semantic backends exist and semantic_policy permits, in which
///    case Execute with empty syntax list — transitional semantic-only path)
/// 3. Otherwise → Execute with all registered syntax backends
/// 4. Semantic backends are included in Execute based on SemanticPolicy:
///    - Disabled → no semantic backends in plan
///    - EnabledWhenAvailable → include registered semantic backends
///    - Required → include semantic backends; if none registered, this is
///      not a planning error (the classifier will determine the final tier)
///
/// The transitional semantic-only path (rules 1-2 exceptions) exists to
/// support TypeScript/Kotlin during Phase 3 migration. It is not a target
/// product state.
pub trait DispatchPlanner: Send + Sync {
    fn plan(
        &self,
        file: &PreparedFile,
        registry: &dyn BackendRegistry,
        context: &DispatchContext,
    ) -> ExecutionPlan;
}
```

## Pipeline Design

The indexing pipeline evolves from the current three-stage model (discover →
parse → persist) to an explicit multi-stage pipeline:

### Stage 1: Discover

Unchanged from current implementation. Walks the repository, detects language,
loads file content. Produces `Vec<PreparedFile>`.

Owner crate: `repo-walker` + `indexer`

### Stage 2: Persist file baseline

File record and content blob are written for every recognized file, regardless
of whether symbol extraction succeeds. This ensures files are always visible in
file tree and content retrieval queries.

Owner crate: `indexer` + `store`

### Stage 3: Dispatch planning

For each `PreparedFile`, the `DispatchPlanner` consults the `BackendRegistry`
and `DispatchContext` to produce an `ExecutionPlan`.

Owner crate: `indexer`

### Stage 4: Syntax extraction

For files with `ExecutionPlan::Execute`, run all planned syntax backends.
Collect results as `Vec<BackendAttempt<SyntaxExtraction, SyntaxError>>`.

Owner crate: `indexer` (orchestration), `syntax-platform` (extraction)

### Stage 5: Syntax merge

Merge successful syntax extractions into a `SyntaxMergeBaseline` via
`MergeEngine::merge_syntax()`. When only one syntax backend succeeded, this
is a trivial pass-through. When none succeeded, the baseline is `None`.

This stage exists so semantic backends receive one consistent merged view
of the file's syntax-derived symbols, not raw per-backend extractions.

Owner crate: `indexer`

### Stage 6: Semantic extraction

For files with semantic backends in their plan, run all planned semantic
backends. Pass the `SyntaxMergeBaseline` (from Stage 5) as input via
`SemanticBackend::enrich_symbols(file, syntax_baseline)`.

When the syntax baseline is `None` (no syntax backend available or all
failed), semantic backends receive `None` and must produce symbols
independently — this is the transitional semantic-only path.

Collect results as `Vec<BackendAttempt<SemanticExtraction, SemanticError>>`.

Owner crate: `indexer` (orchestration), `semantic-*` crates (extraction)

### Stage 7: Final merge

`MergeEngine::merge_final()` takes the syntax baseline and semantic
extractions and produces the canonical `MergeResult` with merged symbols,
provenance, and capability tier.

Owner crate: `indexer`

### Stage 8: Enrich

Summaries, keywords, and metadata enrichment applied to merged symbols. This
stage is unchanged from current behavior.

Owner crate: `indexer`

### Stage 9: Persist symbols + aggregates

Write canonical symbols with provenance. Update file-level and repo-level
aggregates including capability tier.

Owner crate: `indexer` + `store`

### Data flow

```
PreparedFile
    │
    ├──→ [file record + blob persisted]
    │
    ├──→ DispatchPlanner.plan() ──→ ExecutionPlan
    │                                    │
    │    ┌───────────────────────────────┘
    │    │
    │    ├──→ SyntaxBackend.extract_symbols()     ──→ Vec<SyntaxExtraction>
    │    │                                                    │
    │    ├──→ MergeEngine.merge_syntax()           ──→ Option<SyntaxMergeBaseline>
    │    │                                                    │
    │    ├──→ SemanticBackend.enrich_symbols(baseline) ──→ Vec<SemanticExtraction>
    │    │                                                         │
    │    └──→ MergeEngine.merge_final(baseline, semantic) ──→ MergeResult
    │                                                              │
    │    ┌─────────────────────────────────────────────────────────┘
    │    │
    │    ├──→ CapabilityClassifier.classify() ──→ CapabilityTier
    │    │
    │    └──→ enrich() ──→ enriched symbols
    │                           │
    └──→ [symbols + aggregates persisted with provenance + tier]
```

## Core Model Changes

Ticket 2 implements these model changes. The changes are listed here so the
full design is visible in one place.

### New fields on `SymbolRecord`

| Field                 | Type              | Purpose                                       |
|-----------------------|-------------------|-----------------------------------------------|
| `capability_tier`     | `CapabilityTier`  | How this symbol was produced                   |

### New fields on `FileRecord`

| Field                 | Type              | Purpose                                       |
|-----------------------|-------------------|-----------------------------------------------|
| `capability_tier`     | `CapabilityTier`  | Achieved tier for this file                    |

### Replaced fields

| Current field     | Replacement               | Reason                                    |
|-------------------|---------------------------|-------------------------------------------|
| `quality_level`   | `capability_tier`         | More precise; three tiers instead of two  |
| `quality_mix`     | `capability_tier`         | Tier is more useful than percentage mix   |
| `source_adapter`  | `source_backend` (String) | Renamed for consistency with new terminology |

### `SourceSpan` relocation

`SourceSpan` currently lives in `adapter-api`. It moves to `core-model` since
it is used across syntax-platform, semantic-api, and indexer.

### Retained fields

Fields on `SymbolRecord` that remain unchanged: `id`, `repo_id`, `file_path`,
`language`, `kind`, `name`, `qualified_name`, `signature`, `start_line`,
`end_line`, `start_byte`, `byte_length`, `content_hash`, `confidence_score`,
`indexed_at`, `docstring`, `summary`, `parent_symbol_id`, `keywords`,
`decorators_or_attributes`, `semantic_refs`.

## Migration Plan

Migration is phased across Epic 17 tickets. Each phase is a single ticket
that leaves the system in a working state.

### Phase 1: Architecture ratification (Ticket 1 — this document)

- no code changes
- architecture doc finalized and merged
- all downstream tickets implement against this design

### Phase 2: Core model refactor (Ticket 2)

- add `CapabilityTier` enum to `core-model`
- add `capability_tier` to `FileRecord` and `SymbolRecord`
- add `SourceSpan` to `core-model`
- replace `quality_level` with `capability_tier` in persistence layer
- replace `quality_mix` with `capability_tier` on `FileRecord`
- rename `source_adapter` to `source_backend`
- update metrics to report file/syntax/semantic coverage counts
- schema migration for existing data
- existing tests updated; no new backends yet

### Phase 3: Syntax platform + Rust migration (Ticket 3)

- create `syntax-platform` crate with:
  - grammar registry and parser lifecycle
  - `SyntaxBackend` trait implementation infrastructure
  - shared extraction utilities (scope tracking, signature extraction,
    docstring extraction, span conversion)
  - language module pattern (LanguageProfile equivalent)
  - Rust language module (migrated from `adapter-syntax-treesitter`)
- create `semantic-api` crate with `SemanticBackend` trait
- implement `BackendRegistry`, `DispatchPlanner`, `MergeEngine`,
  `CapabilityClassifier` in `indexer`
- update `indexer` pipeline to use new dispatch/extract/merge flow
- adapt existing semantic backends (TypeScript, Kotlin) to new
  `SemanticBackend` trait
- remove `adapter-api` and `adapter-syntax-treesitter` crates
- all existing tests pass on new architecture

**Transitional state after Phase 3:**

| Language     | Syntax | Semantic | Tier                |
|--------------|--------|----------|---------------------|
| Rust         | yes    | no       | SyntaxOnly          |
| TypeScript   | no     | yes      | SemanticOnly        |
| Kotlin       | no     | yes      | SemanticOnly        |
| PHP          | no     | no       | FileOnly            |
| Python       | no     | no       | FileOnly            |
| Go           | no     | no       | FileOnly            |
| Java         | no     | no       | FileOnly            |
| JavaScript   | no     | no       | FileOnly            |

TypeScript and Kotlin are classified as `CapabilityTier::SemanticOnly` during
this phase. This is acceptable because:

- their existing semantic behavior is preserved without regression
- the `SemanticOnly` tier accurately represents their state in metrics and
  diagnostics (they are not misclassified as `FileOnly`)
- the `DispatchPlanner` uses the transitional semantic-only path (rules 1-2
  exceptions) to include their semantic backends even without syntax backends
- the `CapabilityClassifier` returns `SemanticOnly` when only semantic
  extractions succeeded

Resolution: adding syntax backends for TypeScript and Kotlin (outside Epic 17
scope, or via follow-up tickets) will promote them to `SyntaxPlusSemantic` and
eliminate all `SemanticOnly` classifications.

### Phase 4: Language expansion (Tickets 4-8)

Each ticket adds one language module to `syntax-platform`:

- Ticket 4: PHP
- Ticket 5: Python
- Ticket 6: Go
- Ticket 7: Java
- Ticket 8: JavaScript

Each language module follows the same pattern:

1. Add tree-sitter grammar dependency
2. Implement `SyntaxBackend` via language profile
3. Register in `BackendRegistry`
4. Add contract tests and language-specific extraction tests
5. Verify file outline and symbol search behavior

### Phase 5: Query surface rework (Ticket 9)

- ensure symbol search and file outline work correctly across all capability
  tiers
- add capability-tier-aware behavior to query responses where useful
- add integration tests asserting tier-specific behavior

### Phase 6: Benchmark update (Ticket 10)

- measure file, syntax, and semantic coverage separately
- validate token/context reduction on PHP/Laravel and other newly supported
  ecosystems
- update benchmark guidance docs

## Routing/Dispatch Diagnostics Design

The replacement for `AdapterRouter` / `AdapterPolicy` diagnostics works as
follows:

### Structured logging

Every file processed emits a structured log event with:

```
file_path, language, plan_type (file_only | execute),
  file_only_reason (if applicable),
  planned_syntax_backends, planned_semantic_backends,
  syntax_outcomes (success | error per backend),
  semantic_outcomes (success | error per backend),
  final_capability_tier
```

### Metrics

| Metric                           | Meaning                                    |
|----------------------------------|--------------------------------------------|
| `files_by_capability_tier`       | Count of files per tier per repo (includes SemanticOnly as distinct tier) |
| `files_file_only_by_reason`      | Count of file-only files by reason          |
| `files_semantic_only`            | Count of files in transitional SemanticOnly state (should trend to zero) |
| `syntax_backend_errors`          | Count of syntax backend failures by backend |
| `semantic_backend_errors`        | Count of semantic backend failures by backend|
| `symbols_by_tier`                | Count of symbols per capability tier        |

### Diagnostic queries

The query engine should support answering:

- "Why is file X file-only?" → `FileOnlyReason` from the execution plan
- "Which files had backend failures?" → files with error entries in
  `BackendAttempt` results
- "What capability tier does language Y achieve?" → aggregate of
  `capability_tier` across files for that language

## Validation Strategy

Three validation targets before declaring the architecture ready:

### 1. Laravel/PHP repository

- prove that a formerly file-only ecosystem now gains useful syntax indexing
- PHP symbols extracted for controllers, models, services, commands, jobs, tests
- useful file outlines
- better token/context behavior than file-only retrieval

### 2. CodeAtlas repository

- validate mixed-language coexistence on the project's own codebase
- no regression in Rust, TypeScript, or Kotlin behavior
- coherent behavior for mixed file/syntax/semantic capability tiers

### 3. Android/Kotlin repository

- validate semantic-over-syntax layering on an existing semantic-supported
  ecosystem
- Kotlin semantic behavior remains strong
- merge/provenance remains deterministic and understandable

## Relationship To Planning Docs

- product/epic framing lives in:
  `docs/planning/universal-syntax-indexing.md`
- issue decomposition lives in:
  `docs/planning/github-issues/universal-syntax-*.md`
- this document is the canonical technical design source for all
  implementation tickets in Epic 17

# Code-Derived Repository Intelligence Notes

Status: Draft architecture notes for the proposed post-Epic-17 repository
intelligence initiative

This document is intentionally narrower than a full architecture spec. It
exists to give the planning and issue drafts a concrete shared vocabulary so
later ticket work does not drift into ambiguous "understanding" language.

## Purpose

Define the minimum architectural concepts needed to build code-derived
repository intelligence from source code and indexed artifacts alone.

## Scope Boundary Notes

For this initiative:

- **language-specific** means logic required to extract or normalize facts from
  a particular programming language grammar or semantic backend
- **framework-specific** means logic that assumes one application framework's
  conventions, runtime wiring model, or directory semantics (for example MVC,
  dependency-injection conventions, route registration conventions, or ORM
  conventions tied to one framework family)
- **ecosystem-agnostic** means the capability remains useful across multiple
  languages and project structures even if the extraction quality differs by
  language

Language-specific extraction is expected in the platform. Framework-specific
enrichment is explicitly not part of the first execution slice for this epic.

## Core Objects

### 1. Code artifact

A code artifact is any persisted retrieval target that can participate in graph
edges, slices, or derived facts.

Examples:

- repository
- module / namespace / package
- file
- symbol
- exact source slice

Not every artifact needs its own top-level table in the first slice, but the
model should treat these as first-class concepts.

### 2. Slice

A slice is a persisted or reconstructible retrieval unit defined by stable
source bounds.

Minimum attributes:

- repo identifier
- file identifier
- start byte
- byte length
- start line
- end line
- slice kind
- evidence anchor (symbol id, edge id, or explicit request)

Preferred slice kinds:

- symbol_body
- enclosing_scope
- supporting_context
- explicit_range
- workflow_bundle_member

### 3. Relationship edge

A relationship edge is a persisted connection between two code artifacts.

Minimum attributes:

- source artifact id
- target artifact id
- edge kind
- derivation method
- confidence
- optional supporting spans or symbol ids

Preferred edge kinds for the first slice:

- contains
- imports
- depends_on
- references
- calls
- implements
- overrides

The system must support partial coverage per language and per edge kind.

### 4. Derived fact

A derived fact is a deterministic conclusion computed from stored artifacts and
edges rather than entered manually.

Minimum attributes:

- fact kind
- subject artifact id
- score or strength
- supporting artifact ids
- supporting edge ids
- reason payload

Examples:

- central_symbol
- likely_coordinator
- write_hotspot
- side_effect_boundary
- public_api_surface
- tightly_coupled_module

Derived facts should remain inspectable and reproducible.

## Query Families

This initiative is expected to add four query families:

### 1. Exact retrieval queries

Return only the source spans needed for a question.

Examples:

- get symbol body
- get enclosing scope
- get supporting slice around callsite
- get explicit file range

### 2. Graph neighbor queries

Return directly connected artifacts.

Examples:

- callers / callees
- references to symbol
- imported modules
- dependency neighbors

### 3. Fact queries

Return persisted structural conclusions.

Examples:

- central symbols in repo
- likely state writers
- likely coordinators
- side-effect boundaries

### 4. Path queries

Return graph paths and supporting evidence bundles.

Examples:

- path from symbol A to symbol B
- likely path from entrypoint to writer
- minimal supporting evidence for a behavior explanation

## Evidence Model

Every higher-level answer in this initiative should be explainable in terms of:

- slices
- edges
- supporting artifacts
- derivation method
- confidence where heuristic logic is involved

The architecture should make it possible to answer:

- what artifacts does this result depend on?
- what edges were traversed?
- what slices were selected?
- what heuristic created this fact?

## First-Slice Constraints

The initial implementation should avoid scope blowouts by following these
constraints:

- slice retrieval lands before broad path or fact features
- graph persistence begins with portable edge kinds
- fact generation remains conservative
- path queries are graph traversals, not natural-language synthesis features
- every new concept has a testable evidence contract

## Non-Goals For These Notes

- final storage schema details
- final MCP tool names
- framework-specific enrichers
- history/PR/issue-derived memory

Those can be decided in later ticket-specific architecture work.

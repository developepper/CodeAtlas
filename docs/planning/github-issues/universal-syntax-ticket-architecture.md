## Problem

The current adapter and routing model reflects a world where broad syntax
indexing is sparse and file-only fallback is common. That is no longer the
target product direction.

Before implementing many new language extractors, CodeAtlas needs a canonical
architecture for file, syntax, and semantic capability tiers and clear
boundaries between syntax backends, semantic backends, and merge policy.

## Scope

- define the capability model (`file`, `syntax`, `semantic`)
- define subsystem boundaries for syntax and semantic indexing
- replace the current unified adapter/routing model with explicit syntax,
  semantic, and merge roles
- define the compatibility stance for this initiative explicitly

## Deliverables

- architecture/planning doc updates
- clear subsystem ownership and terminology
- explicit design decisions for refactor boundaries
- actual proposed Rust-facing replacement interfaces
- explicit crate-boundary and data-flow design
- explicit migration plan for the current Rust syntax path and current unified
  adapter abstraction

## Non-goals

- no production crate restructuring in this ticket
- no partial implementation that leaves the replacement interfaces undefined

## Acceptance Criteria

- [ ] capability tiers are explicitly defined and documented
- [ ] the role of syntax indexing as the platform baseline is explicit
- [ ] the role of semantic indexing as enrichment over syntax is explicit
- [ ] the docs state that clean long-term architecture is favored over backward
      compatibility for this initiative
- [ ] the current unified adapter/routing model is explicitly retired as the
      long-term center of the architecture
- [ ] the replacement architectural split between syntax backends, semantic
      backends, and merge policy is explicit
- [ ] the ticket produces concrete trait/interface proposals rather than a
      second high-level planning pass

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [crates/adapter-api/src/router.rs](crates/adapter-api/src/router.rs)
- [crates/indexer/src/stage.rs](crates/indexer/src/stage.rs)

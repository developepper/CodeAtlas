## Problem

CodeAtlas needs a reusable syntax indexing platform that can support many
languages without devolving into a brittle collection of one-off adapters.

## Scope

- create a multi-language tree-sitter-backed syntax subsystem
- define grammar registration / parser lifecycle
- add shared extraction utilities and language-module patterns
- migrate the current Rust syntax path onto the new subsystem

## Deliverables

- syntax platform implementation
- shared parser/extraction infrastructure
- Rust migrated onto the new subsystem as the first in-tree production language
- tests proving deterministic behavior across languages

## Acceptance Criteria

- [ ] the syntax subsystem supports multiple languages through a common pattern
- [ ] shared parser/extraction utilities exist for new language modules
- [ ] the existing Rust syntax path is folded into the new subsystem rather
      than left on the retired abstraction
- [ ] deterministic spans and symbol outputs are preserved
- [ ] the subsystem is positioned for continued language expansion without
      repeated foundational redesign

## Testing Requirements

- Unit: parser lifecycle, grammar registry, and shared extraction utility coverage
- Integration: Rust indexing runs through the new syntax subsystem with stable output
- Security: N/A
- Performance: validate no material regression in baseline Rust syntax indexing throughput

## Dependencies

- Requires Ticket 1
- May require Ticket 2 first or in parallel if core-model changes are foundational to the new subsystem

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [crates/adapter-syntax-treesitter](crates/adapter-syntax-treesitter)

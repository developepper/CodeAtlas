## Problem

The current canonical model and reporting semantics were shaped around sparse
symbol coverage. Broad syntax indexing will make file-only, syntax, and
semantic capability distinctions first-class product concepts, and the model
must represent that cleanly.

## Scope

- evolve canonical schemas where needed for capability/provenance tiers
- update metrics to distinguish file, syntax, and semantic coverage
- update reporting semantics to reflect the layered capability model
- make the foundational model changes needed for the long-term architecture in
  this first slice rather than deferring them for incremental convenience

## Deliverables

- schema/model changes as needed
- metrics and reporting updates
- regression coverage for capability-tier semantics

## Acceptance Criteria

- [ ] capability/provenance distinctions are represented cleanly in the model
- [ ] metrics distinguish file-only, syntax-indexed, and semantic-indexed output
- [ ] reporting remains honest about symbol quality versus index coverage
- [ ] any schema/index version implications are documented explicitly
- [ ] the ticket does not defer known foundational schema/model corrections
      purely to keep the initial implementation smaller

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)
- [crates/core-model/src/lib.rs](crates/core-model/src/lib.rs)
- [crates/indexer/src/pipeline.rs](crates/indexer/src/pipeline.rs)
- [crates/indexer/src/metrics.rs](crates/indexer/src/metrics.rs)

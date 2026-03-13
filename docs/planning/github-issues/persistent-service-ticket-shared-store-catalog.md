## Problem

The underlying store already supports multiple repos in one database, but the
current product UX still encourages one DB path per repo and the repo catalog
is missing service-oriented status and freshness metadata needed for a
persistent local backend.

## Scope

- preserve the existing multi-repo schema foundation
- add repo catalog fields for registration time, last successful index time,
  indexing status, and freshness/staleness signals
- make shared storage-root usage the canonical product shape
- remove or simplify CLI/indexer assumptions that steer users toward one DB per
  repo as the default workflow
- ensure shared blob/content storage behavior is explicit and tested

## Deliverables

- store migration(s) or rebuild path for the added repo catalog metadata
- shared store APIs for richer repo catalog operations
- canonical shared-storage-root guidance in code/docs/config
- tests covering multiple repos in one store

## Acceptance Criteria

- [ ] one store can represent multiple repos cleanly
- [ ] repo records include enough metadata for registration, indexing, discovery, and freshness workflows
- [ ] CLI/indexer defaults no longer push one-store-per-repo as the canonical local model
- [ ] tests demonstrate isolation between repos in the same shared store

## Testing Requirements

- Unit: store and repo catalog behavior
- Integration: multi-repo persistence and lookup
- Regression: repo-scoped query isolation
- Security: verify repo separation is not weakened by shared-store defaults

## Dependencies

- Parent epic: Epic 13 persistent local service issue
- Depends on the architecture-definition ticket

## Review Checklist

- schema change is simple and explicit
- repo isolation is enforced
- CLI/indexer defaults now match the actual shared-store capability
- rebuild/reindex story is documented

## References

- [docs/planning/persistent-multi-repo-local-service.md](docs/planning/persistent-multi-repo-local-service.md)
- [crates/store/src/migrations.rs](crates/store/src/migrations.rs)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)

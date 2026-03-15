## Problem

Even when file records exist, `get_file_content` currently returns placeholder
empty content and the query layer is not designed around file-only indexed
artifacts as a primary use case.

## Scope

- wire file content retrieval to blob storage
- ensure file tree, file outline, and repo outline work for file-only indexed
  files
- validate service and MCP behavior for repositories that only have file-level
  indexed artifacts

## Deliverables

- query/service wiring for blob-backed file content retrieval
- integration tests for CLI, service, and MCP file-level queries
- explicit user-visible behavior for file outlines with zero symbols

## Acceptance Criteria

- [ ] `get_file_content` returns stored content for indexed recognized files
- [ ] file tree includes file-only indexed files
- [ ] file outline returns file metadata plus an empty symbol list when no symbols exist
- [ ] repo outline reflects file-only indexed files in counts and listings
- [ ] service and MCP query flows work on a recognized-language repo with no current symbol adapter

## Testing Requirements

- Integration: CLI file-content retrieval on file-only indexed repo
- Integration: service and MCP file-content retrieval on file-only indexed repo
- Regression: symbol-bearing repo behavior remains unchanged
- Negative: unknown file paths still return not found

## Dependencies

- Parent epic: TBD
- Depends on: pipeline/store ticket (TBD)

## Review Checklist

- blob retrieval boundary is clean
- file-only indexed repos remain navigable through current query surfaces
- behavior is understandable to AI clients and human CLI users
- no placeholder content remains in the production path

## References

- [docs/planning/recognized-language-file-indexing.md](docs/planning/recognized-language-file-indexing.md)
- [crates/query-engine/src/store_service.rs](crates/query-engine/src/store_service.rs)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)
- [crates/service/src/routes.rs](crates/service/src/routes.rs)
- GitHub issue: TBD

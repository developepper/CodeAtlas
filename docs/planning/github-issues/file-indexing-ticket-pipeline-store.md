## Problem

The parse stage currently records `no adapter available` as a file error and
the persist stage only writes file records for parsed files with adapter
output. Recognized files without symbols therefore disappear from the index.

## Scope

- change parse/persist flow so recognized files survive without symbols
- preserve blob writes and file records for recognized files
- keep true adapter failures distinguishable in diagnostics
- use the same `store::content_hash` content-hash contract for file-only and
  symbol-bearing files
- handle the current `merge_outputs(vec![]) -> None` behavior intentionally so
  recognized files do not disappear through the empty-merge path
- ensure repo/file aggregates remain consistent when file-only indexed records
  are present

## Deliverables

- pipeline changes for recognized file fallback
- persist/store changes for file-only indexed records
- regression coverage for recognized non-Rust repositories

## Acceptance Criteria

- [ ] recognized files without adapters receive file records and stored blobs
- [ ] recognized files with real adapter failures still receive file records and stored blobs
- [ ] missing-adapter cases are not counted as ordinary parse errors
- [ ] file-only indexed records use the same content-hash contract as symbol-bearing files
- [ ] the implementation handles the current empty-merge rejection path explicitly
- [ ] stale-file cleanup remains correct for mixed file-only and symbol-bearing repos
- [ ] tests prove non-empty index behavior on a repository with recognized languages but no current symbol adapter

## Testing Requirements

- Integration: recognized-language repo with no adapter produces file records
- Integration: mixed repo persists both file-only and symbol-bearing files
- Regression: stale cleanup does not delete newly file-only indexed entries
- Diagnostics: missing-adapter and adapter-failure paths remain distinguishable

## Dependencies

- Parent epic: TBD

## Review Checklist

- every recognized discovered file has a clear persistence path
- blob and metadata writes remain consistent
- metrics stay truthful about file-only versus symbol-bearing output
- implementation does not accidentally hide real adapter failures
- the empty-merge path cannot silently drop recognized files

## References

- [docs/planning/recognized-language-file-indexing.md](docs/planning/recognized-language-file-indexing.md)
- [crates/indexer/src/stage.rs](crates/indexer/src/stage.rs)
- [crates/store/src/file_store.rs](crates/store/src/file_store.rs)
- [crates/core-model/src/lib.rs](crates/core-model/src/lib.rs)
- GitHub issue: TBD

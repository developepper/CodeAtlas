## Problem

PHP is currently recognized but file-only in CodeAtlas, which makes Laravel and
other PHP repositories poor fits for symbol-level navigation and token
reduction workflows.

## Scope

- implement production-grade PHP syntax extraction on the new syntax subsystem
- support high-value PHP symbol kinds and relationships
- validate behavior on a Laravel-style repository

## Deliverables

- PHP syntax extraction
- Laravel-oriented integration coverage
- improved file outline and symbol search behavior for PHP repos

## Acceptance Criteria

- [ ] PHP repositories no longer collapse to file-only indexing by default
- [ ] PHP file outline returns meaningful symbols for common Laravel files
- [ ] PHP symbol search returns useful results on a Laravel-style repo
- [ ] Laravel/PHP proving-ground evidence is captured for review

## Testing Requirements

- Unit: PHP symbol extraction coverage for high-value symbol kinds and parent/container relationships
- Integration: Laravel-oriented indexing tests covering outlines and symbol search
- Security: N/A
- Performance: validate PHP syntax extraction remains acceptable on a large Laravel repository

## Dependencies

- Requires Ticket 1
- Requires Ticket 3
- May require Ticket 2 first if model/query changes are needed for PHP symbol representation

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)

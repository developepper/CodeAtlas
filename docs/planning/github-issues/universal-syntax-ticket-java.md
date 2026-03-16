## Problem

Java is a high-value ecosystem for broad syntax coverage and is currently not
treated as a first-class syntax baseline in CodeAtlas.

## Scope

- implement production-grade Java syntax extraction
- add regression coverage for Java

## Deliverables

- Java syntax extraction
- integration and regression tests

## Acceptance Criteria

- [ ] Java repositories gain meaningful symbol coverage through syntax indexing
- [ ] file outline is useful on representative Java repositories
- [ ] symbol search returns useful results on representative Java repositories

## Testing Requirements

- Unit: Java symbol extraction coverage for high-value symbol kinds
- Integration: representative Java repository tests for file outline and symbol search
- Security: N/A
- Performance: validate Java extraction remains acceptable on representative repositories

## Dependencies

- Requires Ticket 1
- Requires Ticket 3

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)

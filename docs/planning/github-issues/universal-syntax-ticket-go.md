## Problem

Go is a common recognized language that remains file-only today. It is a
high-value target for a broad syntax baseline and should be implemented as its
own ticket because its symbol model and extraction complexity should be
estimated independently.

## Scope

- implement production-grade Go syntax extraction
- add regression coverage for Go

## Deliverables

- Go syntax extraction
- integration and regression tests

## Acceptance Criteria

- [ ] Go repositories gain meaningful symbol coverage through syntax indexing
- [ ] file outline is useful on representative Go repositories
- [ ] symbol search returns useful results on representative Go repositories

## References

- [docs/planning/universal-syntax-indexing.md](docs/planning/universal-syntax-indexing.md)

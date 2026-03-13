# GitHub Issue Seeding

This file is historical bootstrap guidance for the initial repository setup.

For current issue creation, use the reviewed issue-body docs under:

- `docs/planning/github-issues/`

Current planned post-v1 issue set:

- `persistent-service-epic.md`
- `persistent-service-ticket-architecture.md`
- `persistent-service-ticket-shared-store-catalog.md`
- `persistent-service-ticket-repo-lifecycle.md`
- `persistent-service-ticket-runtime.md`
- `persistent-service-ticket-mcp-bridge.md`
- `persistent-service-ticket-docs.md`

Use these labels first:

```bash
gh label create epic --color F97316 --description "Outcome-level issue" || true
gh label create ticket --color 2563EB --description "Single-PR deliverable" || true
gh label create manual --color 6B7280 --description "Manual intervention" || true
gh label create ci --color 0EA5E9 --description "CI and automation" || true
gh label create docs --color 14B8A6 --description "Documentation" || true
gh label create rust --color F59E0B --description "Rust implementation" || true
gh label create testing --color 10B981 --description "Testing scope" || true
gh label create security --color EF4444 --description "Security scope" || true
gh label create blocked --color 7C3AED --description "Blocked by dependency" || true
```

Create epics from backlog:

```bash
gh issue create --title "Epic 0: Repository Governance and CI Foundation" --label epic
gh issue create --title "Epic 1: Workspace and Core Model" --label epic
gh issue create --title "Epic 2: Ingestion and Discovery" --label epic
gh issue create --title "Epic 3: Adapter API and Syntax Baseline" --label epic
gh issue create --title "Epic 4: Store and Index Commit Path" --label epic
gh issue create --title "Epic 5: Query Engine" --label epic
gh issue create --title "Epic 6: MCP Server" --label epic
gh issue create --title "Epic 7: Incremental Indexing and Reliability" --label epic
gh issue create --title "Epic 8: Security, Observability, Performance" --label epic
gh issue create --title "Epic 9: Semantic Adapters" --label epic
gh issue create --title "Epic 10: V1 Readiness" --label epic
```

Then create ticket/manual issues from `docs/planning/issue-backlog.md`.

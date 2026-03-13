# Post-V1 Roadmap

This document captures likely product and platform directions for CodeAtlas
after Milestone M10 is complete. Milestone M9 semantic adapter rollout is now
done; this document is intentionally strategic rather than a committed delivery
plan.

## Positioning

With semantic adapters in place and V1 readiness next, CodeAtlas can move from being a
local indexing engine to becoming a general-purpose code intelligence substrate
for agents, developer tools, and platform workflows.

The strongest long-term direction is not just "index more code", but "answer
more useful questions with less context" through deterministic, explainable,
security-conscious retrieval.

An adjacent opportunity is making CodeAtlas legible in economic terms for
agent users: not just correctness and latency, but tokens avoided, context
precision, and repeat-query efficiency over time.

## Strategic Themes

### 1. Agent-Optimized Retrieval

Focus on helping coding agents answer structural and behavioral questions with
less repository scanning and better trust in results.

Potential additions:

- Cross-symbol relationship queries (callers, callees, implementations,
  overrides, imports, exports).
- Confidence-aware retrieval that explains why a result ranked highly.
- Exact-slice source retrieval from persisted content using stable offsets or
  line/byte ranges, so clients can request only the relevant implementation.
- Impact-analysis tools for "what breaks if this changes?"
- Change-planning and dependency-tracing workflows on top of the symbol graph.
- Query explanation metadata so consumers can debug ranking and provenance.
- Retrieval cost metadata such as estimated token savings, result precision, or
  context avoided, as long as the math is deterministic and privacy-safe.

### 2. Multi-Repo and Organization Intelligence

Today the architecture is strongest at the repository level. A natural next
step is expanding into workspace- and organization-level retrieval.

Potential additions:

- Multi-repo indexing and shared search across repositories.
- Cross-repo symbol relationships and dependency graph traversal.
- Ownership and service metadata attached to files, symbols, and repos.
- Fleet-wide discovery such as "where is this concept implemented?"

### 3. Operational UX and Index Lifecycle

The product will become more useful as index freshness and operator confidence
improve.

Potential additions:

- File-system watch mode or daemonized background indexing.
- Safer and richer content retrieval from blob storage with explicit limits.
- Index freshness/status reporting and stale-index detection.
- Adapter health reporting and parse-failure visibility.
- Per-repo configuration for ignore rules, budgets, and adapter policy.
- Better local operator ergonomics around MCP setup, logging, diagnostics, and
  environment validation for common clients and developer environments.

### 4. Connectors and Source Acquisition

Local-first should remain the default, but there is clear value in supporting
 additional trusted ways to ingest source without weakening the security model.

Potential additions:

- Connector-based indexing for Git hosting providers and remote sources.
- Cached mirror/fetch workflows for repositories not present on local disk.
- Drift detection between indexed state and upstream repository state.
- Policy controls that govern whether content, metadata, or only structural
  index artifacts may leave the local machine.

### 5. Semantic and Graph Depth

Beyond initial semantic adapters, the next major quality unlock is richer
program relationships.

Potential additions:

- Broader semantic adapter coverage across high-value ecosystems.
- Confidence-aware merge of syntax and semantic outputs.
- Type, reference, and call graph persistence in the core model.
- Coverage metrics that expose semantic depth by language and repository.
- Better language-specific structure semantics so outlines and retrieval are
  useful across more ecosystems, not just syntactically parseable ones.

### 6. Domain and Ecosystem Enrichment

Repository structure alone is often not enough. Domain-aware enrichment can
help agents understand what a file or symbol means in the surrounding system.

Potential additions:

- Extensible context providers that ingest framework or platform metadata.
- Enrichment for ecosystems such as OpenAPI, Terraform, dbt, Django, Rails, or
  build-system metadata where it materially improves retrieval quality.
- Searchable business or architecture metadata attached to files and symbols.
- Policy controls to keep enrichment deterministic and safe for enterprise use.

### 7. Service Surface and Ecosystem

If CodeAtlas becomes shared infrastructure, integration quality becomes as
important as core indexing quality.

Potential additions:

- First-class HTTP/gRPC API surface for non-MCP consumers.
- IDE/editor integrations backed by the same retrieval engine.
- Out-of-process adapter/plugin boundaries for isolation and extension.
- Hosted control plane capabilities such as auth, quotas, retention, and audit.
- Stronger MCP client experience with documented install flows, diagnostics,
  compatibility guidance, and stable tool contracts across clients.

### 8. Trust, Explainability, and Enterprise Readiness

If the platform is used in larger organizations, retrieval quality alone will
not be enough; policy and transparency will matter.

Potential additions:

- Stronger privacy controls around telemetry, retention, and export policy.
- Benchmark scorecards and benchmark corpus publication.
- Retrieval explainability and evidence traces for each result set.
- Compatibility, migration, and SLO reporting surfaced as product features.
- Optional local-model or private-model enrichment paths that preserve data
  residency while improving summaries or metadata quality.

## Candidate Post-V1 Epics

These are not committed issues, but they are plausible backlog seeds.

### Epic 11: Agent Retrieval and Relationship Queries

- Add callers/callees/reference/implementation query APIs.
- Add impact-analysis and dependency-tracing query primitives.
- Add ranking explanation metadata to query responses.
- Add precise symbol/file slice retrieval with explicit byte or line bounds.
- Add deterministic retrieval-efficiency metadata for clients and benchmarks.

### Epic 12: Watch Mode and Index Operations

- Add background watch mode / daemonized local indexing.
- Add index health, freshness, and failure reporting.
- Add per-repo indexing policy configuration.
- Add MCP/client diagnostics and operator-facing setup validation.

Planning note:

- The local-service initiative in
  `docs/planning/persistent-multi-repo-local-service.md` intentionally pulls in
  the minimal Epic 12 concerns required for repo lifecycle, freshness, and
  service health.

### Epic 13: Multi-Repo Intelligence

- Add multi-repo storage/query model extensions.
- Add cross-repo search and dependency traversal.
- Add ownership/service metadata integration.
- Add connector-backed acquisition for remote repositories and mirrors.

Planning note:

- The concrete first execution slice for Epic 13 is documented in
  `docs/planning/persistent-multi-repo-local-service.md`.
- That slice focuses on the persistent multi-repo local service model.
- Cross-repo search, dependency traversal, ownership metadata, and remote
  connectors remain plausible follow-on work after the local service baseline.

### Epic 14: Service APIs and Integrations

- Add HTTP/gRPC service surface.
- Add IDE/editor integration path.
- Add plugin/out-of-process adapter boundary.
- Add ecosystem/domain enrichment provider framework and initial providers.

### Epic 15: Trust, Explainability, and Enterprise Controls

- Add retrieval evidence/explanation payloads.
- Add stronger policy controls for telemetry/retention/export.
- Add benchmark scorecards, SLO reporting, and audit-facing operations docs.
- Add privacy-preserving local/private model enrichment options where useful.

## Commercial Model Principles

CodeAtlas can remain open source and still support a healthy business model if
monetization is aligned to convenience, support, and organizational value
rather than basic developer access.

Guiding principles:

- Keep individual developer access low-friction and affordable in practice,
  especially for local indexing, core retrieval, and self-directed use.
- Charge where the work meaningfully consumes ongoing time, infrastructure, or
  operational responsibility.
- Prefer monetizing support, reliability, hosted capability, enterprise
  controls, and high-touch integration over monetizing the right to experiment.
- Be explicit about what stays free so adoption is based on trust, not fear of
  future lock-in.
- Favor models that improve access to better tooling for individual developers
  while asking companies to pay when they derive organizational value.

Likely free/community surface:

- Local indexing and self-hosted individual use.
- Core MCP/CLI retrieval workflows.
- Standard syntax adapters and baseline documentation.
- Community contribution and extension paths.

Likely paid/commercial surface:

- Priority support, troubleshooting, onboarding, and implementation help.
- Managed or shared hosted deployments.
- Enterprise administration features such as auth, RBAC, audit, policy, and
  compliance-oriented controls.
- Premium operational workflows, org-scale capabilities, or high-cost
  integrations.
- Consulting or custom adapter/integration work for teams with specialized
  requirements.

This model is intended to be fair in two directions:

- Fair to maintainers by treating support, infrastructure, and deep integration
  work as valuable labor.
- Fair to users by preserving meaningful free access for developers who want to
  use, learn from, and build on the project.

## Prioritization Guidance

If the goal is fastest product value after V1, prioritize in this order:

1. Agent retrieval and relationship queries.
2. Multi-repo local service baseline, watch mode, and operational UX.
3. Broader multi-repo intelligence and connectors.
4. Service APIs, ecosystem integrations, and enrichment.
5. Enterprise, explainability, and policy expansions.

If the goal is enterprise adoption sooner, move trust controls and hosted
service capabilities earlier.

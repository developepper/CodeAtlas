# Persistent Multi-Repo Local Service Plan

Status: Complete — Epic 13 (#148-#154) delivered and closed

Owner intent: make CodeAtlas usable as one persistent local code-intelligence
backend across many repositories and AI clients, while preserving a clean path
to future centralized/team-hosted deployments.

## Why This Exists

Today, CodeAtlas is easiest to use as:

- a per-repository local index
- a per-process stdio MCP server
- a tool launched directly by an AI client

That works, but it creates friction for the intended daily workflow:

- a developer works across multiple repositories
- an AI client should be able to use one CodeAtlas installation repeatedly
- switching repositories should not require mentally tracking multiple running
  MCP processes or separate DB paths
- persistent indexes should survive client restarts and machine sessions

The desired product shape is:

- one long-running local CodeAtlas service
- one persistent storage root
- many indexed repositories in one catalog
- repo-aware queries through stable `repo_id` values
- AI clients using the same CodeAtlas backend across projects

This document is the review artifact for that direction and is intentionally
written so its Epic and ticket sections can later be promoted directly into
GitHub issues.

Tracked GitHub issues:

- Epic: #148
- Ticket 1: #149
- Ticket 2: #150
- Ticket 3: #151
- Ticket 4: #152
- Ticket 5: #153
- Ticket 6: #154

## Product Decision

CodeAtlas should evolve toward one core multi-repo service architecture with
two deployment modes that share the same core model:

1. Self-hosted local-first service for individual developers and small teams.
2. Managed centralized service for business/team deployments.

These modes should live in tandem, not as competing product directions.

## Strategic Position

This direction is a product-model change first and a packaging choice second.

Important distinction:

- Docker can be a good packaging/deployment option for a persistent local
  service.
- Docker is not the core product decision.
- The core decision is moving from "spawn per repo/process" toward
  "persistent multi-repo service."

## Relationship To Existing Roadmap

This initiative should be treated as the concrete planning document for the
local-service portion of post-v1 roadmap Epic 13: Multi-Repo Intelligence.

Relationship to existing roadmap themes:

- Epic 13 provides the main strategic slot because this work makes multi-repo
  operation a first-class product model.
- A narrow subset of Epic 12: Watch Mode and Index Operations is pulled in
  where required for repo lifecycle, freshness, and service health.
- Docker packaging is not part of the core Epic 13 slice and should be treated
  as a follow-up once the persistent service architecture is validated.

Relationship to current backlog state:

- `docs/planning/issue-backlog.md` still shows Epic 10: V1 Readiness as open,
  even though the progress note says milestones M0-M11 are complete.
- This document assumes the persistent local service is the first major
  post-v1 architecture initiative.
- If Epic 10 artifacts are truly incomplete, either finish the remaining
  readiness docs first or explicitly re-baseline the backlog before execution.

## Design Principles For This Initiative

1. Optimize for the real daily workflow: one developer, many repos, repeated
   AI usage.
2. Preserve local-first trust by default.
3. Keep repo identity explicit and stable.
4. Maintain hosted-ready boundaries instead of baking local-only assumptions
   into the core architecture.
5. Prefer correctness and clear architecture over backward compatibility.

## Refactor Policy For This Project Stage

This project is early enough that backward compatibility should not be treated
as a primary constraint for this initiative.

Direction:

- prefer refactoring immediately when the current shape blocks the correct
  architecture
- do not preserve awkward interfaces solely to avoid breakage for hypothetical
  users
- remove or redesign incorrect boundaries early rather than accreting tech debt
- if an intentional breaking change is made, document the new canonical shape
  clearly and update all adjacent docs/tests in the same ticket

This is not permission for careless churn. It is a directive to choose the
correct architecture when the existing one is only a temporary v1 shape.

## User Workflows To Support

### Workflow A: Single developer across multiple local repos

1. User starts one CodeAtlas service.
2. User adds or indexes repo `alpha`.
3. User adds or indexes repo `beta`.
4. User points AI client(s) at the same CodeAtlas backend.
5. AI queries specify or discover the target `repo_id`.
6. User switches projects without standing up another CodeAtlas instance.

### Workflow B (Future): Docker-based local infrastructure

1. User runs one CodeAtlas container with a mounted storage volume.
2. User mounts local repositories into the container or provides host paths.
3. CodeAtlas stores indexes/content in the persistent volume.
4. AI tooling connects to the same running backend across sessions.

### Workflow C: Future team-hosted deployment

1. A managed CodeAtlas service stores many repositories for many users.
2. Requests are tenant-scoped and authenticated.
3. The same core repo catalog, index lifecycle, and query model apply.

## What Must Change

### Current dominant model

- one DB path usually associated with one repo
- stdio MCP server launched directly by the client
- process lifetime tied to a single invocation

### Target model

- one service instance owns one storage root
- one catalog tracks many repos
- indexing adds or refreshes repos inside that shared store
- query paths remain repo-scoped where appropriate
- transport is suitable for a long-running service

## Key Product Decisions

### 1. Multi-repo is the canonical local model

The dominant local user story should become:

- "CodeAtlas is my local code-intelligence backend"

not:

- "I create a separate CodeAtlas runtime per repository."

### 2. `repo_id` remains a first-class boundary

Multi-repo support should not hide repo identity.

Requirements:

- every repo in the catalog has a stable `repo_id`
- repository-scoped query tools continue to accept `repo_id`
- service UX should make discovering valid repos simple

### 3. Stdio alone is insufficient for the persistent-service story

The current stdio MCP server is useful and should remain supported for simple
client compatibility, but it is not the right sole transport for a persistent
daemon/service.

Likely direction:

- keep stdio MCP as a compatibility surface
- add a long-running local service mode with a service-suitable transport
- if needed, provide a thin MCP-facing gateway or launcher that connects AI
  clients to the long-running service

Recommended direction:

- use HTTP for the persistent local service, most likely via `axum`
- keep stdio MCP for direct-client compatibility
- add a thin MCP bridge/launcher that translates MCP tool calls into service
  requests

Why this is the current recommendation:

- HTTP is the simplest widely understood fit for a long-running daemon
- health/status endpoints and local operator diagnostics are straightforward
- a Dockerized local deployment maps naturally to HTTP
- an HTTP service boundary can later be reused or adapted for hosted/service
  deployment
- the bridge approach lets MCP compatibility remain additive instead of forcing
  the entire persistent service to inherit stdio process-lifecycle assumptions

Important tradeoff:

- adopting `axum` for the persistent service likely introduces `tokio` as a
  first-class runtime dependency for the service path
- this is a real architectural shift from the current mostly synchronous MCP
  server approach
- the trade is acceptable if it buys a cleaner daemon model, clearer health
  surfaces, and better long-running service ergonomics
- Ticket 1 should confirm that this async-runtime expansion is intentionally
  scoped to the service path rather than leaking unnecessarily across the whole
  workspace

Alternatives considered but not preferred for the first slice:

- Unix domain sockets: attractive for local-only use, but less portable and
  more awkward as the primary cross-platform story
- gRPC: plausible later, especially for hosted/internal service boundaries, but
  heavier than needed for the first persistent local-service slice

### 4. Docker is a packaging option, not a product requirement

Docker should be evaluated as:

- a convenient local deployment path
- a runtime dependency bundling path
- a persistent storage packaging path

Docker should not force architecture decisions that make the non-container local
experience worse.

## Recommended Architecture Direction

### Service Shape

Add a persistent local service mode with:

- one storage root
- one repo catalog
- index-management commands
- query APIs over the shared store
- health/status reporting

### Storage Shape

The current metadata schema already supports multiple repos in one store via a
`repos` table keyed by `repo_id`, with repo-scoped `files` and `symbols`
records. The real gap is not "make the store multi-repo" but "make the product
model and operational metadata match that capability."

The first slice should focus on:

- making a shared storage root the canonical local default
- stopping the CLI/indexer UX from steering users toward one DB per repo
- adding missing repo-catalog metadata such as:
  - registration timestamp
  - last successful index timestamp
  - indexing status (`pending`, `indexing`, `ready`, `failed`)
  - freshness/staleness signals
- tightening service-level repo catalog operations around the existing store
  model

### Transport Shape

Support two layers:

1. A persistent service transport for the long-running local server.
2. A client-facing compatibility layer for AI tools that expect MCP.

This allows:

- a clean local service story
- continued MCP compatibility
- future hosted/service transport reuse

### CLI Evolution

The current CLI is direct-store oriented and requires `--db <path>` for query
commands. The persistent-service model needs an explicit CLI migration story.

Recommended first-slice direction:

- keep existing direct-store CLI commands working during the transition
- add service-oriented commands and flags explicitly rather than silently
  auto-detecting a daemon
- avoid auto-detection in the first slice because it makes operational behavior
  ambiguous and harder to debug

Likely shape:

- direct mode remains available for low-level/local workflows
- service mode becomes the canonical user path
- a future cleanup ticket may remove or simplify direct-store entrypoints once
  the service model is proven

Recommended client-facing UX for the first slice:

- the user runs one persistent local service, for example something like
  `codeatlas serve --data-root <path>`
- AI clients continue to use an MCP command in their config
- the MCP command acts as a bridge process that proxies tool calls to the local
  HTTP service
- that bridge should be explicit in implementation but mostly transparent in
  user setup

Desired outcome:

- users do not configure one separate CodeAtlas process per repo
- users do not need to know HTTP details unless they are doing local operator
  work
- AI client configuration remains close to today's MCP mental model

### Deployment Shape

The same service architecture should support:

- native local process
- future hosted/team deployment

## Non-Goals For The First Slice

- Full hosted/team product implementation.
- Auth/RBAC/billing in the local-first slice.
- Multi-tenant controls in the local-first slice.
- Broad query-surface expansion unrelated to service architecture.
- Premature compatibility preservation for old per-repo assumptions.

## Open Decision Record

These decisions still need explicit confirmation, but they are no longer
blank-slate questions. The default implementation assumptions should be:

1. Transport for the persistent local service:
   recommended answer: local HTTP service via `axum`
2. AI client connection model:
   recommended answer: MCP bridge/launcher talks to the HTTP service
3. Canonical storage root for local mode:
   recommended answer: one user-scoped CodeAtlas data directory, not per-repo
   `.codeatlas/` DB paths as the canonical model
4. Repo identity rules:
   recommended answer: stable service-owned `repo_id`, with collision handling
   explicit at registration time
5. CLI migration model:
   recommended answer: keep direct `--db` flows temporarily, add explicit
   service commands/flags, avoid auto-detect in the first slice
6. Runtime model:
   recommended answer: accept `tokio` as a service-path dependency if HTTP via
   `axum` remains the chosen transport, but keep async concerns contained to the
   service/bridge boundary where practical

Remaining open detail:

- exact HTTP surface and endpoint design
- exact local storage-root path convention
- exact repo registration semantics for moved/renamed source roots
- whether direct MCP-to-service transport is needed later beyond the bridge
- exact CLI command names for the service and MCP bridge entrypoints

## Proposed Epic

### Title

Epic 13: Persistent Multi-Repo Local Service

### Objective

Make CodeAtlas usable as one persistent local code-intelligence backend across
many repositories and AI clients, while preserving an architecture that can be
extended into a managed centralized service later.

### Problem

The current per-repo/per-process model creates daily friction for developers
who work across multiple repositories and want one stable AI integration point.
It also leaves Docker and future hosted deployment as awkward fits because the
main transport is currently stdio MCP tied to a single spawned process.

### In Scope

- define the persistent multi-repo local service product model
- add shared multi-repo storage/catalog support
- add lifecycle operations for registering, indexing, refreshing, and removing
  repos in one service
- define and implement a transport model suitable for a long-running local
  service
- preserve or adapt MCP compatibility for AI clients
- document the architectural relationship between self-hosted local and future
  centralized deployment

### Out Of Scope

- full hosted/team feature implementation
- auth/RBAC/quotas/billing
- organization/workspace UI
- remote repo connectors unless directly required for the new local model
- unrelated retrieval feature expansion

### Epic Definition Of Done

- a user can run one persistent CodeAtlas instance and index/query multiple
  repos through it
- repository identity and discovery are clear and stable
- AI client integration does not require one separate CodeAtlas runtime per repo
- the architecture cleanly supports both self-hosted local and future managed
  deployment modes
- Docker support is explicitly deferred or split into follow-up work unless it
  proves necessary to validate the core service model
- docs reflect the new canonical local usage model

### Review Evidence Required

- architecture/design doc updates merged
- acceptance evidence per child ticket
- end-to-end proof of multi-repo indexing and querying
- explicit note on transport choice and rationale
- explicit note on CLI migration shape and rationale

## Proposed Ticket Breakdown

Each ticket below is intentionally self-contained so it can be lifted into a
new session or GitHub issue with minimal additional context.

### Ticket 1

#### Title

Ticket: Define the persistent multi-repo local service architecture

#### Problem

The repository currently documents local-first and hosted-ready boundaries, but
it does not define the canonical architecture for a persistent multi-repo local
service or how that service relates to MCP and future centralized deployment.

#### Scope

- define the canonical local service model
- define how it differs from the current per-repo/per-process model
- confirm or adjust the recommended HTTP-plus-MCP-bridge transport direction
- define how MCP compatibility fits into the new model
- define the CLI migration strategy from direct `--db` usage to service-first
  usage
- define the architectural relationship between local self-hosted and future
  centralized deployments
- document the explicit refactor-first policy for this initiative

#### Deliverables

- architecture doc for the persistent local service
- updated deployment-mode guidance reflecting the new canonical local model
- explicit decision record for transport and service boundaries
- explicit decision record for CLI migration boundaries

#### Acceptance Criteria

- the canonical local product shape is documented unambiguously
- the doc explains how multi-repo local and future centralized modes share one
  architectural direction
- the transport strategy is explicit enough to unblock implementation tickets
- the CLI migration strategy is explicit enough to unblock service and client
  integration tickets
- the docs state that correctness/refactor quality is favored over backward
  compatibility for this early-stage initiative

#### Testing Requirements

- no code tests required
- review must confirm the document is actionable enough to start implementation
  without hidden assumptions

#### Dependencies

- none

#### Review Checklist

- service boundaries are clear
- transport decision is explicit
- hosted relationship is clear
- no accidental commitment to unsupported features

### Ticket 2

#### Title

Ticket: Make shared-store usage canonical and add missing repo catalog metadata

#### Problem

The underlying store already supports multiple repos in one database, but the
current product UX still encourages one DB path per repo and the repo catalog
is missing service-oriented status/freshness metadata needed for a persistent
local backend.

#### Scope

- preserve the existing multi-repo schema foundation
- add repo catalog fields for registration time, last successful index time,
  indexing status, and freshness/staleness signals
- make shared storage-root usage the canonical product shape
- remove or simplify CLI/indexer assumptions that steer users toward one DB per
  repo as the default workflow
- ensure shared blob/content storage behavior is explicit and tested

#### Deliverables

- store migration(s) or rebuild path for the added repo catalog metadata
- shared store APIs for richer repo catalog operations
- canonical shared-storage-root guidance in code/docs/config
- migration or rebuild strategy appropriate for this early-stage refactor
- tests covering multiple repos in one store

#### Acceptance Criteria

- one store can represent multiple repos cleanly
- repo records include enough metadata for registration, indexing, discovery,
  and freshness workflows
- CLI/indexer defaults no longer push one-store-per-repo as the canonical local
  model
- tests demonstrate isolation between repos in the same shared store

#### Testing Requirements

- unit tests for store/repo catalog behavior
- integration tests for multi-repo persistence and lookup
- regression tests for repo-scoped query isolation
- security tests to verify repo separation is not weakened by shared-store
  defaults

#### Dependencies

- Ticket 1

#### Review Checklist

- schema is simple and explicit
- repo isolation is enforced
- CLI/indexer defaults now match the actual shared-store capability
- rebuild/reindex story is documented

### Ticket 3

#### Title

Ticket: Add repo catalog and lifecycle operations for a persistent local service

#### Problem

Users need service-level operations for adding, listing, refreshing, and
removing repos. Without a repo catalog UX, a shared service becomes a hidden
database rather than a usable multi-project tool.

#### Scope

- add repo registration/indexing commands or APIs
- add repo listing/discovery operations
- add refresh/reindex operations per repo
- add removal/de-registration operations
- expose repo status, source root, last indexed time, and health/freshness
  basics

#### Deliverables

- repo lifecycle command/API surface
- repo listing/status output
- documentation for repo registration and switching workflows

#### Acceptance Criteria

- a user can add multiple repos to one CodeAtlas instance
- a user can list all indexed/known repos
- a user can refresh or remove a repo without affecting others
- lifecycle operations surface registration state, index state, and freshness
  clearly
- repo discovery is clear enough that AI tooling or wrappers can choose the
  correct `repo_id`

#### Testing Requirements

- integration tests for add/list/refresh/remove flows
- negative tests for duplicate/replaced/conflicting repo identity cases

#### Dependencies

- Ticket 2

#### Review Checklist

- lifecycle operations are complete enough for real daily use
- repo identity collisions are handled intentionally
- status output is understandable

### Ticket 4

#### Title

Ticket: Implement a persistent local service runtime

#### Problem

A persistent multi-repo product needs a real long-running service process with
clear startup, storage-root ownership, lifecycle handling, and diagnostics.

#### Scope

- add a long-running local service mode
- define canonical startup/stop behavior
- configure service storage root and runtime state
- add HTTP health/status behavior appropriate to the chosen transport
- ensure logs/diagnostics are operationally usable

#### Deliverables

- runnable persistent local service entrypoint
- startup/configuration docs
- health/status behavior

#### Acceptance Criteria

- one CodeAtlas instance can stay running across multiple repo workflows
- startup configuration is explicit and documented
- service health can be checked without digging through internals
- diagnostics are clear on startup and runtime failure

#### Testing Requirements

- integration tests for service startup/shutdown
- runtime tests for shared-store operation over multiple repos
- diagnostics tests for invalid storage/config cases
- performance checks to avoid obvious service startup overhead regressions

#### Dependencies

- Ticket 1
- Ticket 2

#### Review Checklist

- service runtime is stable
- storage root ownership is clear
- diagnostics are actionable

### Ticket 5

#### Title

Ticket: Adapt AI client integration for the persistent service model

#### Problem

The current AI integration story centers on stdio MCP launched directly by the
client. A persistent service needs a bridge model so AI clients can use the
long-running backend without spawning isolated per-repo instances.

#### Scope

- implement the MCP-to-service bridge model
- preserve a practical MCP compatibility story
- ensure repo-scoped operations remain clear in multi-repo usage
- avoid leaking transport-specific complexity into core query logic

#### Deliverables

- MCP bridge/launcher path for the persistent service
- updated MCP/service docs
- end-to-end example flow from AI client to multi-repo backend
- explicit recommended client configuration shape for the first slice

#### Acceptance Criteria

- AI clients can use the persistent service without one CodeAtlas runtime per
  repo
- the integration path is documented clearly enough for real setup
- the intended user-facing MCP setup flow is explicit, not implicit
- repo targeting/discovery is understandable in the client workflow
- the implementation preserves clean separation between transport and query
  semantics
- bridge behavior is explicit rather than hidden in ambiguous auto-detection

#### Testing Requirements

- integration tests for the chosen client/service path
- subprocess or service tests covering handshake/query flows as applicable
- regression tests to ensure stdout/log discipline where MCP compatibility
  remains relevant

#### Dependencies

- Ticket 1
- Ticket 3
- Ticket 4
- Ticket 3

#### Review Checklist

- client setup is actually simpler than the current model
- repo selection is practical
- transport boundary stays clean
- the bridge UX does not create more operator friction than it removes

### Ticket 6

#### Title

Ticket: Update docs and canonical usage guidance for the persistent local model

#### Problem

Once the persistent multi-repo service exists, the docs must stop presenting
per-repo/per-process usage as the dominant mental model and must explain the
CLI migration path clearly.

#### Scope

- update README usage guidance
- update deployment docs
- update operations runbook
- update roadmap/backlog references as needed
- document the direct-store vs service-first CLI distinction during the
  transition
- keep docs explicit about what is implemented vs future hosted direction

#### Deliverables

- updated user-facing docs
- updated operator-facing docs
- review notes summarizing the new canonical flow

#### Acceptance Criteria

- docs present the persistent local service as the canonical local model
- docs explain how multi-repo usage works
- docs explain the CLI migration story clearly enough to avoid user confusion
- docs explain the relationship between local self-hosted and future managed
  deployment
- docs remain honest about unsupported hosted features

#### Testing Requirements

- no code tests required
- manual doc review for accuracy against implementation

#### Dependencies

- Tickets 1 through 5 as applicable

#### Review Checklist

- user journey is easy to follow
- no stale per-repo assumptions remain in primary docs
- local vs future hosted boundaries are still clear

## Deferred Follow-Up

### Docker Packaging

Docker remains a plausible and potentially valuable local deployment option, but
it should not be part of the first implementation epic for the persistent local
service.

Reason for deferral:

- the core architecture risk is the service/runtime/transport model
- Docker does not reduce that risk
- the first slice should validate the native local service model before adding
  packaging/distribution surface area

If the service model is validated, Docker can become a follow-up ticket with:

- container packaging
- persistent-volume guidance
- repo mount/registration guidance
- local compose examples

## Tracking And Review Model

Every implementation ticket created from this document should include:

- problem statement
- scope
- explicit deliverables
- acceptance criteria
- testing requirements
- dependencies
- review checklist
- direct doc/code references

Review should track:

- architecture correctness
- simplification of user workflow
- removal of obsolete assumptions
- quality of operational/documentation surface

## Suggested Delivery Sequence

1. Ticket 1: architecture definition
2. Ticket 2: canonical shared-store usage and repo catalog metadata
3. Tickets 3 and 4 in parallel: repo catalog/lifecycle plus persistent runtime
4. Ticket 5: AI client integration
5. Ticket 6: final doc updates

This sequence should be adjusted only if the transport decision requires
earlier prototyping.

Dependency note:

- Ticket 4 should depend on Tickets 1 and 2.
- Ticket 3 and Ticket 4 can proceed in parallel once the architecture and
  shared-store assumptions are settled.
- Ticket 5 depends on the runtime and enough repo catalog behavior to support a
  credible multi-repo client flow.

## Exit Criteria For Sign-Off

This plan is ready to convert into GitHub issues when:

- the product direction is agreed: persistent multi-repo local service is the
  canonical local model
- the relationship to future hosted deployment is agreed
- the refactor-first policy is accepted
- the ticket set is judged complete enough to execute one PR per issue
- each ticket contains enough context to begin work in a fresh session

## References

- `README.md`
- `docs/architecture/deployment-modes.md`
- `docs/architecture/mcp-server-planning.md`
- `docs/architecture/rust-code-intelligence-plan.md`
- `docs/planning/post-v1-roadmap.md`
- `docs/planning/issue-backlog.md`
- `docs/workflow/github-process.md`
- `crates/store/src/migrations.rs`

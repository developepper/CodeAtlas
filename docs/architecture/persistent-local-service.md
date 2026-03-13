# Persistent Local Service Architecture

Status: Active architecture baseline for Epic 13 (#148)

This document defines the canonical architecture for the persistent multi-repo
local CodeAtlas service. It finalizes the decisions outlined in the planning
document (`docs/planning/persistent-multi-repo-local-service.md`) and provides
the concrete technical direction needed to unblock implementation tickets
#150-#154.

## Purpose

CodeAtlas is evolving from a per-repo/per-process tool to a persistent local
code-intelligence backend that serves many repositories through one running
instance. This document records the architectural decisions that govern that
transition.

## Canonical Local Product Shape

The canonical local deployment of CodeAtlas is:

- one long-running service process
- one storage root directory containing a shared SQLite database and blob store
- one repo catalog tracking many indexed repositories
- repo-scoped queries through stable `repo_id` values
- AI clients connecting through an MCP bridge process that proxies to the
  service

The per-repo/per-process model (`codeatlas mcp serve --db <path>`) remains
supported during the transition but is no longer the canonical product shape.

## Architecture Overview

```
+------------------+     stdio MCP      +------------------+
|   AI Client      | <----------------> |   MCP Bridge     |
| (Claude, Cursor) |                    |                  |
+------------------+                    +--------+---------+
                                                 |
                                            HTTP |
                                                 v
                                        +--------+---------+
                                        | CodeAtlas Service |
                                        +--------+---------+
                                                 |
                                        +--------+---------+
                                        |  Shared Store     |
                                        |  - metadata DB    |
                                        |  - blob store     |
                                        +-------------------+
                                        | repo: alpha       |
                                        | repo: beta        |
                                        | repo: gamma       |
                                        +-------------------+
```

### Components

**CodeAtlas Service** (`codeatlas serve`): A long-running HTTP server that owns
a storage root and exposes query and repo-management APIs. This is the primary
runtime for multi-repo local usage.

**MCP Bridge** (`codeatlas mcp bridge`): A thin stdio process that AI clients
launch. It translates MCP tool calls into HTTP requests to the running service.
This preserves the MCP client configuration model that users already know.

**Shared Store**: A single storage root directory containing the SQLite metadata
database and content blob store. Multiple repositories coexist in one database
via the existing `repo_id`-scoped schema.

**Direct MCP Server** (`codeatlas mcp serve --db <path>`): The existing stdio
MCP server. Remains available for simple single-repo workflows and backward
compatibility during the transition.

## Decision Records

### DR-1: Transport for the persistent local service

**Decision:** HTTP via `axum`.

**Rationale:**
- HTTP is the simplest widely-understood fit for a long-running daemon.
- Health/status endpoints and operator diagnostics are straightforward.
- A Dockerized local deployment maps naturally to HTTP.
- The HTTP boundary can later be reused for hosted/team deployment.
- The MCP bridge approach keeps MCP compatibility additive rather than forcing
  the service to inherit stdio process-lifecycle assumptions.

**Tradeoff:** This introduces `tokio` as a first-class runtime dependency for
the service path. This is acceptable because:
- The async runtime is scoped to the service and bridge entrypoints.
- The core store, query-engine, and indexer crates remain synchronous.
- `axum` + `tokio` are well-maintained and widely adopted in the Rust ecosystem.

**Alternatives not chosen:**
- Unix domain sockets: less portable, more awkward as the primary cross-platform
  story.
- gRPC: heavier than needed for the first persistent local-service slice.

**Scope:** The `tokio` runtime dependency should be contained to the service
crate (`codeatlas serve`) and the MCP bridge crate. It must not leak into
`store`, `query-engine`, `indexer`, `core-model`, or `adapter-*` crates.

### DR-2: AI client connection model

**Decision:** MCP bridge/launcher that proxies stdio MCP tool calls to the HTTP
service.

**Rationale:**
- AI clients already expect to configure an MCP command in their settings.
- The bridge keeps that configuration nearly identical to today's model.
- Users do not need to know about HTTP unless they are doing operator/debug
  work.
- The bridge is explicit in implementation but mostly transparent in user setup.

**Illustrative user-facing configuration shape:**

Current model (one process per repo):
```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "codeatlas",
      "args": ["mcp", "serve", "--db", "/repo/.codeatlas/index.db"]
    }
  }
}
```

Service model (one bridge to shared service, exact args TBD in ticket #153):
```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "codeatlas",
      "args": ["mcp", "bridge"]
    }
  }
}
```

The bridge connects to the local service at the default address (see DR-5).
An explicit flag or environment variable can override it.

### DR-3: Shared storage root for the service path

**Decision:** The persistent service owns one shared storage root directory,
not per-repo `.codeatlas/` DB paths.

**Direction:** The service should use a user-scoped data directory (e.g.
`~/.codeatlas/` on Unix/macOS, or a platform-appropriate equivalent). The
exact default path, environment variable override, and cross-platform
conventions should be finalized during implementation (ticket #150/#152).

**Proposed layout:**
```
<data-root>/
  metadata.db          # shared SQLite database for all repos
  blobs/               # content-addressed blob storage
```

**Rationale:**
- A shared storage root is the natural home for a multi-repo catalog.
- Per-repo `.codeatlas/` directories remain usable for direct-store workflows
  but are not the service-path default.

**Open detail for implementation tickets:**
- Exact default path and platform conventions (ticket #150).
- Whether and how `codeatlas index` defaults change to point at the shared
  root, or whether only service-mode commands use it (ticket #150).
- Service runtime files (PID, logs) and their location (ticket #152).

### DR-4: Repo identity

**Decision:** Stable service-owned `repo_id` with collision handling at
registration time.

**Rules:**
- `repo_id` is derived from the source root directory name by default (matching
  today's behavior).
- If a collision occurs at registration time (same `repo_id`, different source
  root), the service rejects the registration with a clear error and guidance.
- Users can specify an explicit `repo_id` at registration time to resolve
  collisions.
- `repo_id` values are immutable once registered. Renaming requires
  de-registration and re-registration.

**Rationale:**
- Directory-name derivation is simple and predictable.
- Collision handling at registration time is explicit rather than silently
  overwriting.
- Immutable `repo_id` keeps query results stable across sessions.

### DR-5: Service address and discovery

**Decision:** The service listens on localhost with a fixed default port. The
bridge and service-mode CLI commands discover the service through explicit
configuration, not auto-detection.

**Architectural requirements:**
- The service binds to `127.0.0.1` only (no network exposure by default).
- A configurable bind address and port (via flag and/or environment variable).
- Clients (bridge, CLI) use the same default or an explicit override.
- No auto-detection or service discovery in the first slice.

**Rationale:**
- A fixed default address simplifies zero-config local usage.
- Explicit configuration avoids the operational ambiguity of auto-detect.

**Open detail for implementation tickets:**
- Exact default port number (ticket #152).
- Exact flag and environment variable names (tickets #152/#153).

### DR-6: CLI migration model

**Decision:** Keep direct-store CLI commands working during the transition. Add
explicit service-oriented commands and flags. No auto-detection.

**Architectural requirements:**
- A service startup command (conceptually `codeatlas serve` or similar).
- Repo lifecycle commands for add/list/refresh/remove operations.
- An MCP bridge command that proxies to the running service.
- All existing direct-store commands (`index`, `search-symbols`,
  `mcp serve --db`, etc.) continue to work unchanged.
- No silent auto-detection of whether a service is running; service-mode
  commands are explicitly distinct from direct-store commands.

**Rationale:**
- Explicit commands avoid ambiguity about whether a service is involved.
- No auto-detection in the first slice because it makes operational behavior
  harder to debug.
- Legacy commands are not removed; they continue to work for low-level and
  direct-store workflows.
- A future cleanup ticket may simplify or remove direct-store entrypoints once
  the service model is proven.

**Open detail for implementation tickets:**
- Exact CLI command names and flag shapes (tickets #151/#152/#153).

### DR-7: Refactor-first policy

**Decision:** Correctness and clean architecture are favored over backward
compatibility for this initiative.

**Rules:**
- Refactor immediately when the current shape blocks the correct architecture.
- Do not preserve awkward interfaces solely to avoid breakage for hypothetical
  users.
- Remove or redesign incorrect boundaries early rather than accreting tech debt.
- If an intentional breaking change is made, document the new canonical shape
  clearly and update all adjacent docs/tests in the same ticket.

**Rationale:** This project is early enough that backward compatibility should
not be a primary constraint. The user base is small and the cost of carrying
wrong abstractions forward is higher than the cost of clean breaks now.

## HTTP API Surface Direction

The service HTTP API is internal to the local machine. It does not need to be
stable across versions in the first slice.

The API should cover three categories:

1. **Health and diagnostics:** A health-check endpoint and a status endpoint
   exposing service metadata (version, uptime, storage root, repo count).

2. **Repo catalog operations:** Endpoints for registering, listing, inspecting,
   refreshing, and removing repositories.

3. **Query:** Endpoints mirroring the existing MCP tool surface
   (`search_symbols`, `get_symbol`, etc.). Query endpoints should accept the
   same parameters as the existing MCP tool handlers and return the same
   response envelopes. The MCP bridge translates MCP `tools/call` requests
   into the corresponding HTTP query call.

Exact endpoint paths, request/response schemas, and error conventions are
implementation details for tickets #151 and #152.

## Relationship to Hosted/Centralized Deployment

The persistent local service and a future hosted deployment share the same
architectural direction:

| Concern | Local service | Future hosted |
|---------|--------------|---------------|
| Storage | Local SQLite + disk blobs | Managed DB + object storage |
| Transport | HTTP (localhost) | HTTP/gRPC (network) |
| Auth | None | Tenant-scoped authn/authz |
| Repo catalog | User-managed | Organization-managed |
| Lifecycle | User starts/stops | Platform-managed |
| Query model | Same | Same |
| Tool surface | Same | Same + access controls |

The core crate boundaries (`store`, `query-engine`, `server-mcp`, `indexer`)
remain transport-agnostic and deployment-agnostic. The service layer adds
transport, lifecycle, and operational concerns on top.

A future hosted deployment would:
- Replace `MetadataStore` (SQLite) with a managed database backend
- Replace `BlobStore` (local disk) with object storage
- Add auth middleware to the HTTP layer
- Add tenant isolation to the query layer
- Reuse the same `QueryService` trait, tool registry, and response envelopes

## Async Runtime Containment

The `tokio` runtime required by `axum` must be contained to the service
boundary. The containment strategy:

**Async (tokio-dependent):**
- `codeatlas serve` entrypoint
- `codeatlas mcp bridge` entrypoint
- HTTP request handlers in the service crate
- HTTP client in the bridge crate

**Synchronous (no tokio dependency):**
- `store` crate (SQLite operations)
- `query-engine` crate (query dispatch)
- `server-mcp` crate (tool registry and business logic)
- `indexer` crate (indexing pipeline)
- `core-model` crate (types and validation)
- `adapter-*` crates (language analysis)
- `repo-walker` crate (file discovery)

The service HTTP handlers call into `QueryService` methods synchronously (or
via `spawn_blocking` if needed). This preserves the existing synchronous
contract across all core crates.

## New Crate Structure

The implementation will likely add one or two new crates:

```
crates/
  service/           # HTTP service runtime (axum, tokio)
                     # Repo catalog management APIs
                     # Health/status endpoints
                     # Service lifecycle (startup, shutdown, PID)
  mcp-bridge/        # MCP-to-HTTP bridge process (optional separate crate,
                     # could also live as a module in cli or server-mcp)
```

Whether `mcp-bridge` is a separate crate or a module in `cli` is an
implementation detail to be decided in ticket #153. The important constraint is
that the bridge logic does not pull `axum`/`tokio` server dependencies into
unrelated crates.

## Repo Catalog Metadata

The existing `repos` table schema supports multi-repo storage but lacks
service-oriented metadata. Ticket #150 will add:

| Field | Type | Purpose |
|-------|------|---------|
| `registered_at` | TEXT (ISO 8601) | When the repo was added to the catalog |
| `last_indexed_at` | TEXT (ISO 8601) | Last successful index completion |
| `indexing_status` | TEXT | `pending`, `indexing`, `ready`, `failed` |
| `source_root` | TEXT | Absolute path to the repository on disk |

The existing `indexed_at` field on the `repos` table may be repurposed or
replaced by `last_indexed_at` depending on implementation convenience. The
schema migration strategy is documented in ticket #150.

## What This Document Does Not Decide

- Exact default storage root path and cross-platform conventions (ticket #150).
- Exact HTTP endpoint paths, request/response schemas, and error conventions
  (tickets #151/#152).
- Exact CLI command names, flag shapes, and help text (tickets #151/#152/#153).
- Exact default port number and environment variable names (ticket #152).
- Repo registration semantics for moved/renamed source roots (ticket #151).
- Whether direct MCP-to-service transport (e.g., streamable HTTP MCP) is needed
  later beyond the bridge model.
- Docker packaging (explicitly deferred per planning doc).
- Hosted/team deployment implementation details.

## References

- Planning document: `docs/planning/persistent-multi-repo-local-service.md`
- Deployment modes: `docs/architecture/deployment-modes.md`
- MCP server planning: `docs/architecture/mcp-server-planning.md`
- Post-v1 roadmap: `docs/planning/post-v1-roadmap.md`
- Current store schema: `crates/store/src/migrations.rs`
- Current CLI: `crates/cli/src/main.rs`
- Epic: #148
- This ticket: #149

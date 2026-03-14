# Deployment Modes and Control Boundaries

This document describes the CodeAtlas deployment models and the control
boundaries preserved in the implementation.

It is a companion to the canonical specification:
- `docs/specifications/rust-code-intelligence-platform-spec.md`

For the persistent local service architecture, see:
- `docs/architecture/persistent-local-service.md`

## Purpose

CodeAtlas is implemented as a local-first system. The repository, crate
boundaries, and operational controls are intentionally shaped so hosted
deployment can be added without redesigning the core indexing/query model.

This document answers:

- what deployment modes exist and which is canonical
- what is local-only versus hosted-ready
- which controls apply in each mode
- what assumptions operators should make when running the system

## Current Supported Modes

### Persistent Local Service (canonical)

This is the canonical local deployment model, implemented in Epic 13
(#148-#154). The architecture is documented in
`docs/architecture/persistent-local-service.md`.

Characteristics:

- one long-running CodeAtlas service process per developer machine
- shared storage root (`~/.codeatlas/`) for multiple repositories
- HTTP transport for service communication (localhost only, port 52337)
- MCP bridge process so AI clients connect to the service
- repo catalog with lifecycle operations (add, list, status, refresh, remove)
- no hosted auth, tenancy, or remote storage required

Service entry points:

- `codeatlas serve` (start the persistent HTTP service)
- `codeatlas repo add <path>` (register and index a repository)
- `codeatlas repo list` (list registered repositories)
- `codeatlas repo status <repo_id>` (inspect a repository)
- `codeatlas repo refresh <repo_id>` (re-index a repository)
- `codeatlas repo remove <repo_id>` (de-register and clean up)
- `codeatlas mcp bridge` (MCP bridge for AI clients)

### Direct-Store (legacy, still supported)

The direct-store mode remains available for simple single-repo workflows and
low-level operations. It does not require the persistent service.

Direct-store entry points:

- `codeatlas index`
- `codeatlas search-symbols`
- `codeatlas get-symbol`
- `codeatlas file-outline`
- `codeatlas file-tree`
- `codeatlas repo-outline`
- `codeatlas quality-report`
- `codeatlas mcp serve --db <path>` (stdio MCP server for AI clients)

### Hosted-Ready Architecture Path

This mode is not implemented yet as a deployable product, but the current code
preserves boundaries needed for it.

The persistent local service architecture is designed so the same core crates
and query model can be reused in a hosted deployment. See the "Relationship to
Hosted/Centralized Deployment" section in
`docs/architecture/persistent-local-service.md` for details.

Hosted-ready assumptions:

- repository content remains treated as sensitive and untrusted
- storage boundaries can move from local-only to managed backing services
- request-serving paths can enforce tenant-scoped auth and quotas
- telemetry/export policy can differ from local-only defaults
- adapter isolation can evolve from in-process / child-process execution toward
  stronger sandboxing or out-of-process plugin boundaries

Not implemented yet:

- hosted HTTP/gRPC service surface beyond the local service
- authn/authz
- tenant isolation and quotas
- managed object storage or hosted database deployment
- operational dashboards and alerting stack

## Current Runtime Topology

### Local indexing and query path

1. `repo-walker` discovers files with ignore and security filters.
2. `indexer` routes files to syntax and semantic adapters.
3. `store` persists metadata and content blobs locally.
4. `query-engine` answers lookup and search requests from the local index.
5. CodeAtlas exposes the query surface through:
   - the persistent HTTP service (`codeatlas serve`)
   - the MCP bridge (`codeatlas mcp bridge`) for AI client integration
   - CLI query commands (`search-symbols`, `get-symbol`, etc.)
   - the direct stdio MCP server (`codeatlas mcp serve`) for legacy use

### Semantic adapter processes

Semantic adapters are isolated as subprocesses where required by the language
runtime:

- TypeScript:
  - `tsserver`
- Kotlin:
  - `java` + Kotlin bridge JAR

Current discovery behavior in local mode:

- TypeScript:
  - `TSSERVER_PATH`
  - `node_modules/.bin/tsserver`
  - system `PATH`
- Kotlin:
  - `JAVA_HOME/bin/java`
  - system `PATH`
  - `KOTLIN_BRIDGE_JAR`
  - repo-local `.codeatlas/kotlin-bridge.jar`

If semantic runtime dependencies are unavailable, routing falls back to syntax
adapters for languages with `SemanticPreferred` policy.

## Storage and Data Boundaries

### Metadata

Current implementation:

- SQLite metadata store via `store::MetadataStore`

Stored entities:

- repository records
- file records
- symbol records
- aggregate counts and quality mix

### Content

Current implementation:

- content-addressed blob storage on local disk via `store::BlobStore`

Stored content:

- file blobs keyed by content hash

### Storage root

Current default: shared storage root at `~/.codeatlas/` containing one
`metadata.db` for all repos and a shared `blobs/` directory. Override with
`CODEATLAS_DATA_ROOT` or `--data-root` (service) / `--db` (direct commands).

Legacy per-repo paths (`<repo>/.codeatlas/index.db`) still work with
direct-store commands using `--db`.

### Hosted-ready boundary

The storage layer is already separated from indexing/query orchestration, which
allows future replacement with hosted database and object-storage backends
without changing the higher-level query or adapter contracts.

## Security and Privacy Controls

These controls apply now in local mode and should remain baseline requirements
for any hosted deployment.

### Input handling

- source code is treated as sensitive data
- indexed code is treated as untrusted input
- discovery rejects traversal escapes and unsafe symlink behavior
- binary detection and file size caps reduce parser exposure

### Adapter execution

- no repository code execution is allowed
- semantic adapters communicate over structured protocols
- timeout and resource controls apply to subprocess-based semantic adapters
- failure paths degrade to syntax fallback where policy permits

### Telemetry and logging

- structured logging is available with sensitive-field redaction
- OpenTelemetry spans exist across discovery, parse, persist, query, and MCP
- local mode does not require remote telemetry export

### Hosted-ready controls to preserve

- tenant-scoped storage and request isolation
- quota and rate limiting
- audit logging and retention controls
- export/deletion workflows
- stronger sandboxing for adapter execution

## Operational Assumptions by Mode

### Local operators should assume

- the canonical model is one persistent service per developer machine
- the shared store lives at `~/.codeatlas/` unless overridden
- AI clients connect through the MCP bridge, not by spawning per-repo processes
- semantic coverage depends on local runtime availability
- local logs may contain operational metadata but should not contain raw source
  in structured fields
- schema/version upgrades are handled by store migration logic and may require
  reindex decisions based on schema compatibility rules
- the service binds to localhost only (127.0.0.1) with no authentication

### Future hosted operators should assume

- index freshness, retention, and deletion become service responsibilities
- tenant isolation must be enforced in storage and request paths
- telemetry defaults must remain privacy-safe
- connector and remote acquisition policies must be explicit and auditable

## Decision Summary

Current reality:

- CodeAtlas is publishable and actionable as a local-first system with a
  persistent multi-repo service model.
- The persistent local service (Epic 13, #148-#154) is implemented. Architecture
  decisions are recorded in `docs/architecture/persistent-local-service.md`.
- The repository is not yet publishable as a hosted service implementation.

Hosted-ready claim means:

- the code preserves boundaries for future hosted deployment
- not that hosted serving, tenancy, or ops controls are already complete

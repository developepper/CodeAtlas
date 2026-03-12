# MCP Server Product and Implementation Plan

This document captures the implementation plan for making CodeAtlas usable as a
simple local MCP server for mainstream AI clients.

The success criterion is not merely "an MCP binary exists." The success
criterion is: a user can index a repository once, point their AI client at
CodeAtlas with one obvious command, and use the CodeAtlas query surface through
stdio MCP without writing a custom wrapper.

## Product Goal

CodeAtlas should be easy to use with an AI client of the user's choice,
including generic stdio MCP clients and common tools such as Claude Desktop,
Cursor, ChatGPT desktop-class MCP clients, Codex-style agent wrappers, and
similar integrations.

The intended user flow is:

1. install or build `codeatlas`
2. index a repository
3. configure an MCP client to launch `codeatlas mcp serve --db <path>`
4. use CodeAtlas tools from the client without additional glue code

That end-user simplicity is a required part of scope for the first supported
MCP server release.

## Current State

The `server-mcp` crate is a library-only crate. It contains the business logic
for tool dispatch but no transport, no MCP protocol handler, and no
user-facing launch flow.

### What exists

| Component | Location | Status |
|-----------|----------|--------|
| Tool registry and dispatch | `crates/server-mcp/src/registry.rs` | Complete |
| Tool handlers (8 tools) | `crates/server-mcp/src/tools.rs` | Complete |
| Response envelope types | `crates/server-mcp/src/types.rs` | Complete |
| Query engine (QueryService trait) | `crates/query-engine/src/` | Complete |
| SQLite metadata store | `crates/store/src/` | Complete |
| CLI entry point and tracing init | `crates/cli/src/main.rs` | Complete |

### Implemented tools

- `search_symbols`
- `get_symbol`
- `get_symbols`
- `get_file_outline`
- `get_file_content`
- `get_file_tree`
- `get_repo_outline`
- `search_text`

### Existing scope boundary

The registry already handles:

- parameter deserialization
- tool dispatch
- timing metadata
- error wrapping
- response envelope construction

The missing work is transport/protocol integration, user-facing launch UX,
client documentation, and diagnostics.

## Product Requirements

### 1. One obvious launch command

The primary supported launch command should be:

```text
codeatlas mcp serve --db /absolute/path/to/repo/.codeatlas/index.db
```

This should be the documented default because it is easier to explain and
support than a second product-facing binary.

An optional compatibility alias such as `server-mcp --db ...` may also be
shipped, but the CLI subcommand should be treated as canonical.

### 2. Generic stdio MCP compatibility

The server should target the common denominator used by stdio MCP clients:

- JSON-RPC 2.0 framing with `Content-Length`
- `initialize`
- `notifications/initialized`
- `tools/list`
- `tools/call`
- no protocol-irrelevant stdout output
- stderr-only logs and diagnostics

### 3. Local-first operation

The server should:

- run against a local CodeAtlas index
- require no hosted control plane
- require no auth for local single-user operation
- preserve existing local-first security assumptions

### 4. Clear diagnostics

MCP clients often hide process stderr unless setup fails. That makes startup
clarity part of the product contract.

The first release should include:

- clear errors for missing or unreadable `--db`
- clear errors for invalid database/schema mismatch conditions
- deterministic exit behavior on startup failure
- no accidental stdout leakage that corrupts protocol frames

### 5. Client setup documentation

The first supported release is incomplete without copy-paste setup guidance for
real MCP clients. Architecture notes alone are not enough.

Documentation should include:

- the canonical launch command
- minimal config examples for representative clients
- troubleshooting for failed startup and bad DB paths
- explicit statement of what is and is not supported

## What Is Missing

### 1. JSON-RPC 2.0 framing

No request/response framing types or parser exist in the workspace today.

Required types:

```text
JsonRpcRequest  { jsonrpc, id, method, params }
JsonRpcResponse { jsonrpc, id, result | error }
JsonRpcError    { code, message, data }
```

For stdio transport, messages must be `Content-Length` framed JSON over
stdin/stdout, not newline-delimited JSON.

### 2. MCP protocol state machine

The workspace does not yet handle:

- `initialize`
- `notifications/initialized`
- `tools/list`
- `tools/call`

`ToolRegistry::call()` already maps naturally to `tools/call`.

### 3. stdio transport loop

No stdin/stdout read-write loop exists yet.

### 4. User-facing CLI entrypoint

The current CLI in `crates/cli/src/main.rs` does not have an `mcp` command.

This is the preferred place for the primary launch path because it reduces the
number of things users need to install and understand.

### 5. Optional alias binary

There is no `main.rs` in `server-mcp` and no `[[bin]]` target in
`crates/server-mcp/Cargo.toml`.

This is optional for product success, but useful as a compatibility alias or a
thin transport wrapper if the team wants a dedicated executable name.

### 6. Tool input schemas

`tools/list` must return JSON Schema descriptions for each tool's input.

### 7. Client-facing documentation

The README currently explains that users need to wrap the MCP library
themselves. That guidance must be replaced by a supported setup flow once this
work lands.

## Recommended Product Shape

### Canonical entrypoint

Use the existing CLI as the primary product surface:

```text
codeatlas mcp serve --db <path>
```

Reasons:

- easier end-user mental model
- easier packaging and install guidance
- one executable for indexing, querying, and MCP serving
- lower support burden than separate CLI and server binaries

### Secondary entrypoint decision

Do not treat a separate `server-mcp` executable as part of the initial product
requirement.

An alias binary may be added later only if a concrete client compatibility or
packaging issue justifies it.

Implication:

- the canonical and initially supported user-facing command is
  `codeatlas mcp serve --db <path>`
- docs and testing should center on that command
- packaging should optimize for one obvious executable

### Internal structure

Keep `server-mcp` focused on MCP tool registry and business logic. Implement the
stdio transport layer outside that crate in a thin internal module or crate
that is invoked by the CLI.

This is the chosen default because it preserves a clean boundary between:

- tool/query semantics
- transport/protocol concerns

The important distinction is:

- implementation location is an engineering decision
- `codeatlas mcp serve` is a product decision

Implication:

- `server-mcp` remains reusable as a library surface
- stdio-specific framing and process lifecycle logic do not need to live inside
  the core registry crate
- future transports remain easier to add

## Implementation Approach

### Recommended protocol strategy

Hand-roll the minimal JSON-RPC + MCP subset needed for stdio support.

This remains the best fit because:

- the protocol surface required for v1 is small
- the workspace is already synchronous
- it avoids introducing async runtime and SDK dependency weight
- the team retains control over stdout/stderr discipline

If MCP scope grows materially later, this choice can be revisited.

## Implementation Plan

### Phase 1: Canonical launch UX

Add an `mcp` command family to the CLI with:

- `codeatlas mcp serve --db <path>`

This phase should also decide:

- whether the CLI shares tracing init directly
- whether an alias binary is worth shipping in the same milestone

### Phase 2: Protocol and transport

Implement:

- JSON-RPC request/response/error types
- `Content-Length` framing parser and serializer
- MCP request router
- graceful EOF handling
- stderr-only diagnostics

Supported methods for the first release:

- `initialize`
- `notifications/initialized`
- `tools/list`
- `tools/call`

### Phase 3: Tool schemas

Add JSON Schema definitions for all existing tools based on the request structs
in `crates/server-mcp/src/tools.rs`.

This is part of the first supported release, not follow-up.

### Phase 4: Diagnostics and failure behavior

Add clear startup and runtime behavior for:

- missing `--db`
- unreadable DB path
- open failure
- invalid or incompatible schema
- invalid tool params
- unknown tool name

All diagnostics must remain off stdout.

### Phase 5: Documentation and client guidance

Update docs to make the supported flow explicit:

- build/install
- index once
- configure MCP client to run `codeatlas mcp serve --db ...`
- verify the server starts
- troubleshoot common setup failures

Documentation should include representative config snippets for a small set of
real clients, rather than remaining fully abstract.

The compatibility promise should remain generic: any stdio MCP client that
speaks the supported subset should work. But the first supported release should
still validate and document a small set of concrete clients so end-user setup is
copy-pasteable.

Recommended first documentation set:

- Claude Desktop
- Cursor
- one ChatGPT/Codex-style local MCP client or wrapper with stable config shape

The exact third client can be chosen based on what is stable and in active use
at implementation time.

### Phase 6: Verification

Add:

- unit tests for framing, parsing, routing, and error serialization
- subprocess integration tests over stdio framing
- smoke test for `initialize -> tools/list -> tools/call`
- assertion that stdout contains only protocol frames

## Crate and Code Placement

There are two reasonable implementation patterns:

1. Add the transport implementation to `server-mcp` and call it from
   `codeatlas mcp serve`.
2. Create a thin stdio server module or crate and invoke it from the CLI.

Either is acceptable. The stronger requirement is that the user-facing command
remain simple and stable.

## Acceptance Criteria

The work should be considered complete when all of the following are true:

1. A user can run `codeatlas mcp serve --db <path>`.
2. A generic stdio MCP client can complete `initialize`, `tools/list`, and
   `tools/call`.
3. All existing CodeAtlas MCP tools are exposed with input schemas.
4. Server logs and diagnostics never corrupt stdout protocol frames.
5. The README documents a copy-pasteable supported MCP setup flow, including a
   small set of real client examples.
6. Integration tests cover framed stdio communication with a real subprocess.

## Non-Goals for This Slice

- hosted MCP serving
- HTTP/gRPC APIs
- auth, tenancy, quotas, or billing
- multi-user session management
- dynamic tool registration
- broader MCP features beyond the minimal tool-serving subset

## Recommended First Slice

The smallest useful slice that still matches the product goal is:

1. add `codeatlas mcp serve --db <path>`
2. implement JSON-RPC framing
3. implement `initialize`
4. implement `notifications/initialized`
5. implement `tools/list` with static schemas
6. implement `tools/call` by delegating to `ToolRegistry::call()`
7. add subprocess integration tests
8. add README setup guidance for generic stdio MCP clients

That is enough to make CodeAtlas meaningfully usable with real AI clients
without introducing a larger SDK or hosted-service surface.

## References

- [MCP specification](https://spec.modelcontextprotocol.io/)
- `crates/server-mcp/src/lib.rs`
- `crates/server-mcp/src/registry.rs`
- `crates/server-mcp/src/tools.rs`
- `crates/cli/src/main.rs`
- `docs/architecture/deployment-modes.md`

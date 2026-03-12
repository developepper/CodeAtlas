# MCP Server Binary — Planning

This document captures the gap analysis and implementation plan for adding a
standalone stdio MCP server binary to CodeAtlas.

## Current State

The `server-mcp` crate is a **library-only** crate. It contains all the
business logic for tool dispatch but no transport or protocol handling.

### What exists

| Component | Location | Status |
|-----------|----------|--------|
| Tool registry and dispatch | `crates/server-mcp/src/registry.rs` | Complete |
| Tool handlers (8 tools) | `crates/server-mcp/src/tools.rs` | Complete |
| Response envelope types | `crates/server-mcp/src/types.rs` | Complete |
| Query engine (QueryService trait) | `crates/query-engine/src/` | Complete |
| SQLite metadata store | `crates/store/src/` | Complete |
| Tracing and structured logging | `crates/cli/src/logging.rs` | Complete |

### Implemented tools

- `search_symbols` — ranked symbol search by name
- `get_symbol` — retrieve a single symbol by ID
- `get_symbols` — batch retrieval by IDs
- `get_file_outline` — symbols in a file
- `get_file_content` — file source code
- `get_file_tree` — file listing for a repo
- `get_repo_outline` — repository structure and counts
- `search_text` — full-text search fallback

### How the registry works today

```rust
// ToolRegistry takes a &dyn QueryService and dispatches by tool name
let db = store::MetadataStore::open(&db_path)?;
let svc = StoreQueryService::new(&db);
let registry = ToolRegistry::new(&svc);

let response: McpResponse = registry.call("search_symbols", params_json);
```

The registry handles parameter deserialization, timing, error wrapping, and
metadata envelope construction. A transport layer only needs to route incoming
requests to `registry.call()` and serialize the response back.

This is the key scope boundary: the missing work is transport/protocol
integration, not core query logic or tool semantics.

## What Is Missing

### 1. JSON-RPC 2.0 framing

MCP uses JSON-RPC 2.0 as its message format. No request/response message types
or parsing exist in the workspace today.

Required types:

```
JsonRpcRequest  { jsonrpc, id, method, params }
JsonRpcResponse { jsonrpc, id, result | error }
JsonRpcError    { code, message, data }
```

For stdio transport, messages should be treated as `Content-Length` framed
JSON-RPC payloads over stdin/stdout, not newline-delimited JSON.

### 2. MCP protocol state machine

The MCP protocol lifecycle messages are not handled anywhere:

- `initialize` — client sends capabilities, server responds with server info
  and supported capabilities
- `notifications/initialized` — client notification confirming handshake
- `tools/list` — client requests available tools; server returns tool schemas
- `tools/call` — client invokes a tool by name with arguments

The existing `ToolRegistry::call()` maps directly to the `tools/call` handler.
`ToolRegistry::tool_names()` provides the data for `tools/list`, but tool
parameter schemas (JSON Schema for each tool's input) would need to be added
or derived.

Operational constraints:

- notifications must not receive responses
- stdout must contain protocol frames only
- logs and diagnostics must go to stderr

### 3. stdio transport

No stdin/stdout read-write loop exists. MCP stdio transport should read and
write framed JSON-RPC messages. Implementation requires:

- `Content-Length` frame parsing and serialization
- `BufReader<Stdin>` / `BufWriter<Stdout>`
- stdout protocol-only output discipline
- stderr-only logging
- Graceful EOF handling

### 4. Binary target

No `main.rs` in the server-mcp crate. No `[[bin]]` section in its Cargo.toml.

### 5. Tool input schemas

`tools/list` must return JSON Schema descriptions of each tool's parameters.
The registry knows tool names but does not currently expose parameter schemas.

This should be treated as a required deliverable for the first binary, not a
follow-up.

## Implementation Plan

### Scope estimate

| Piece | Approx. lines |
|-------|---------------|
| JSON-RPC message types and parsing | 200–300 |
| MCP protocol handler (initialize, tools/list, tools/call) | 200–300 |
| stdio transport loop | 100–200 |
| Binary entry point (arg parsing, store init, tracing) | ~100 |
| Tool input schema definitions | ~150 |
| Integration tests | 200–300 |
| **Total** | **~800–1100** |

### Approach options

**Option A: Hand-roll a minimal JSON-RPC + MCP handler**

The MCP stdio protocol is simple for a single-connection server. The full
message set needed is small (initialize, tools/list, tools/call). This avoids
new dependencies beyond `serde` and `serde_json` (already in the workspace).

No async runtime is required — a synchronous stdin read loop is sufficient for
a single-connection stdio server.

Pros:
- Zero new dependencies
- Full control over behavior
- Small surface area to maintain

Cons:
- Must track MCP spec changes manually
- Must write JSON Schema definitions for tool inputs by hand

**Option B: Use a Rust MCP SDK (e.g. `rmcp` or `mcp-rs`)**

An MCP SDK handles JSON-RPC framing, protocol lifecycle, and tool schema
registration. The integration work reduces to registering tools and wiring
the query service.

Pros:
- Less code to write and maintain
- Automatic protocol compliance
- Schema generation may be built in

Cons:
- Adds an external dependency (and likely an async runtime like `tokio`)
- SDK maturity and maintenance varies
- More dependency weight for a narrow use case

### Recommended approach

Option A (hand-rolled) is the better fit for this project. The protocol surface
is small, the workspace is already synchronous, and avoiding an async runtime
keeps the binary lean. If the MCP spec grows significantly in the future, this
decision can be revisited.

### Binary initialization chain

```
1. Parse CLI args
   --db <path>           (required: path to index database)
   --log-level <level>   (optional: tracing verbosity)
   --otel                (optional: enable OpenTelemetry export)
   --read-only           (optional, future-friendly)

2. Initialize tracing
   - Prefer local tracing init or a small shared support module
   - Do not make the server binary depend on the `cli` crate

3. Open storage
   - store::MetadataStore::open(&db_path)

4. Create query service
   - query_engine::StoreQueryService::new(&db)

5. Create tool registry
   - server_mcp::ToolRegistry::new(&svc)

6. Enter stdio loop
   - Read framed JSON-RPC request from stdin
   - Parse method name
   - Route: initialize → return server info
           notifications/initialized → no response
           tools/list  → return tool schemas
           tools/call  → delegate to ToolRegistry::call()
   - Serialize JSON-RPC response
   - Write framed JSON-RPC response to stdout, flush
   - On EOF or error, shut down cleanly
```

### Crate placement

Two options:

1. **Add a `[[bin]]` target to `server-mcp`** — keeps the transport close to the
   registry it wraps. This is the preferred starting point because the binary
   is thin and tightly coupled to the library crate.

2. **Create a new `server-mcp-stdio` crate** — cleaner separation. The binary
   crate depends on `server-mcp`, `store`, and `query-engine`, plus any shared
   logging support if that is extracted later. This becomes attractive only if
   transport/runtime concerns grow beyond a thin wrapper.

### Testing strategy

- **Unit tests**: JSON-RPC parsing, MCP message routing, error serialization.
- **Integration tests**: Spawn the binary as a subprocess, send JSON-RPC
  messages over stdin/stdout, assert correct framed responses. Use a temporary
  SQLite database with fixture data.
- **E2E smoke test**: Index a small repo, start the server, run a tools/list →
  tools/call sequence, verify structured output.

## Prerequisites

- Tool parameter JSON Schemas need to be defined for each of the 8 tools.
  These can be derived from the existing parameter structs in
  `crates/server-mcp/src/tools.rs` or written by hand.
- Decide whether minimal tracing init should live in the binary crate directly
  or in a small shared support module.

## Recommended First Slice

1. add a `[[bin]]` target to `server-mcp`
2. add JSON-RPC + MCP request/response types
3. implement `initialize`
4. implement `notifications/initialized`
5. implement `tools/list` with static schemas
6. implement `tools/call` by delegating to `ToolRegistry::call()`
7. add subprocess integration tests over stdio framing

That slice is enough to make CodeAtlas usable by real MCP clients without
pulling in a larger SDK or introducing hosted-service concerns.

## References

- [MCP specification](https://spec.modelcontextprotocol.io/)
- `crates/server-mcp/src/lib.rs` — current public API
- `crates/server-mcp/src/registry.rs` — ToolRegistry implementation
- `crates/cli/src/main.rs` — reference initialization chain
- `docs/architecture/deployment-modes.md` — deployment context

# MCP GitHub Issues

This document captures the proposed GitHub epic and ticket bodies for the
planned CodeAtlas MCP server work. It translates
`docs/architecture/mcp-server-planning.md` into executable issue tracking.

## Epic

### Title

Epic: First-Class Local MCP Server for AI Clients

### Objective

Make CodeAtlas usable as a simple local stdio MCP server for mainstream AI
clients through the canonical command `codeatlas mcp serve --db <path>`.

### In Scope

- add the canonical CLI entrypoint `codeatlas mcp serve --db <path>`
- implement the minimal stdio MCP subset required for real clients
- expose all existing MCP tools through `tools/list` and `tools/call`
- add clear startup/runtime diagnostics that do not corrupt stdout framing
- add subprocess integration coverage for framed stdio communication
- document copy-paste setup guidance for a small set of real MCP clients

### Out Of Scope

- hosted MCP serving
- HTTP/gRPC APIs
- auth, tenancy, quotas, or billing
- multi-user session management
- dynamic tool registration
- MCP features beyond the minimal tool-serving subset needed for v1
- broader query-surface expansion unrelated to MCP serving

### Child Tickets

- Ticket: Add `codeatlas mcp serve` canonical CLI entrypoint and server wiring
- Ticket: Implement stdio JSON-RPC framing and MCP request routing
- Ticket: Add MCP tool schemas for all existing CodeAtlas tools
- Ticket: Add MCP diagnostics and subprocess integration coverage
- Ticket: Publish supported MCP client setup and troubleshooting docs

### Epic Definition Of Done

- A user can run `codeatlas mcp serve --db <path>` against a local CodeAtlas
  index.
- A generic stdio MCP client can complete `initialize`, `tools/list`, and
  `tools/call`.
- All existing CodeAtlas MCP tools are exposed with input schemas.
- Server diagnostics never corrupt stdout protocol frames.
- README and supporting docs include copy-paste setup guidance for a small set
  of real MCP clients.
- Integration tests cover framed stdio communication with a real subprocess.

### References

- `docs/architecture/mcp-server-planning.md`
- `docs/architecture/deployment-modes.md`
- `README.md`
- `crates/server-mcp/src/lib.rs`
- `crates/server-mcp/src/registry.rs`
- `crates/server-mcp/src/tools.rs`
- `crates/cli/src/main.rs`

## Tickets

### Title

Ticket: Add `codeatlas mcp serve` canonical CLI entrypoint and server wiring

### Problem

There is no supported end-user command for launching CodeAtlas as an MCP server.
Without a canonical CLI entrypoint, setup remains wrapper-dependent and harder
to document or support.

### Scope

- add an `mcp` command family to the CLI
- add `codeatlas mcp serve --db <path>` as the canonical launch path
- validate required CLI arguments and startup preconditions
- wire the command to a reusable MCP server runner
- keep stdio transport code outside the core `server-mcp` registry crate
- ensure server startup and shutdown behavior is deterministic

### Acceptance Criteria

- [ ] `codeatlas mcp serve --db <path>` is a valid CLI command
- [ ] startup fails clearly when `--db` is missing or unreadable
- [ ] the CLI path invokes shared MCP server logic rather than duplicating tool registry behavior
- [ ] `server-mcp` remains focused on reusable tool registry/business logic
- [ ] command help/usage text reflects the new `mcp` command family

### Testing Requirements

- Unit: argument parsing and validation tests for the new CLI path
- Integration: command startup behavior for valid and invalid DB paths
- Security: verify logs/diagnostics stay off stdout during startup failures
- Performance: not required beyond avoiding unnecessary startup overhead

### Dependencies

- None

### Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

### References

- `docs/architecture/mcp-server-planning.md`
- `crates/cli/src/main.rs`
- `crates/server-mcp/src/lib.rs`

---

### Title

Ticket: Implement stdio JSON-RPC framing and MCP request routing

### Problem

The current workspace has MCP tool dispatch logic but no stdio transport, no
JSON-RPC framing, and no MCP protocol handler. Real MCP clients cannot talk to
CodeAtlas until that layer exists.

### Scope

- add JSON-RPC request/response/error types
- implement `Content-Length` framing parser and serializer
- implement MCP request routing for `initialize`,
  `notifications/initialized`, `tools/list`, and `tools/call`
- delegate `tools/call` to the existing `ToolRegistry`
- guarantee protocol-only stdout output
- handle EOF and malformed request cases cleanly

### Acceptance Criteria

- [ ] the server accepts and emits `Content-Length` framed JSON-RPC messages over stdio
- [ ] `initialize` returns server capabilities and tool-serving support
- [ ] `notifications/initialized` is handled without sending a response
- [ ] `tools/call` delegates to `ToolRegistry::call()`
- [ ] malformed requests return JSON-RPC/MCP-compatible errors without crashing the process
- [ ] stdout contains protocol frames only

### Testing Requirements

- Unit: framing parser/serializer, request parsing, response/error serialization
- Integration: handshake and tool-call flows over stdio against a spawned subprocess
- Security: verify malformed input does not leak non-protocol output or raw sensitive details
- Performance: basic framing loop should avoid unnecessary buffering or repeated allocations where practical

### Dependencies

- Ticket: Add `codeatlas mcp serve` canonical CLI entrypoint and server wiring

### Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

### References

- `docs/architecture/mcp-server-planning.md`
- `crates/server-mcp/src/registry.rs`
- `crates/server-mcp/src/types.rs`
- `docs/specifications/rust-code-intelligence-platform-spec.md`

---

### Title

Ticket: Add MCP tool schemas for all existing CodeAtlas tools

### Problem

`tools/list` requires JSON Schema input definitions for each tool. The current
registry exposes tool names but not schemas, so clients cannot reliably render
or validate tool inputs.

### Scope

- define JSON Schema input metadata for all existing MCP tools
- ensure the schema set matches the parameter structs in
  `crates/server-mcp/src/tools.rs`
- return those schemas from `tools/list`
- include stable names, descriptions, required fields, and optional fields
- avoid inventing new tools or changing existing tool semantics in this ticket

### Acceptance Criteria

- [ ] every existing CodeAtlas MCP tool is included in `tools/list`
- [ ] each tool has a JSON Schema input definition matching its current parameters
- [ ] required versus optional fields are represented correctly
- [ ] tool names in `tools/list` match the names accepted by `ToolRegistry`
- [ ] schema output is stable and suitable for snapshot-style testing

### Testing Requirements

- Unit: schema generation/serialization tests per tool or grouped snapshot tests
- Integration: `tools/list` response includes all tools and schemas through the stdio server path
- Security: ensure schemas do not expose internal-only implementation details
- Performance: not required

### Dependencies

- Ticket: Implement stdio JSON-RPC framing and MCP request routing

### Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

### References

- `docs/architecture/mcp-server-planning.md`
- `crates/server-mcp/src/tools.rs`
- `crates/server-mcp/src/registry.rs`

---

### Title

Ticket: Add MCP diagnostics and subprocess integration coverage

### Problem

Even a correct protocol implementation is hard to operate if startup failures,
DB problems, or malformed requests are poorly surfaced. MCP clients also need
high-confidence subprocess coverage because stdio framing bugs are easy to miss
with unit tests alone.

### Scope

- harden startup/runtime diagnostics for missing DB, unreadable DB, and schema/open failures
- ensure all diagnostics remain off stdout
- add subprocess integration tests that cover framed stdio behavior end to end
- add smoke coverage for `initialize -> tools/list -> tools/call`
- assert that invalid requests are reported predictably

### Acceptance Criteria

- [ ] startup failures provide actionable stderr diagnostics for missing or unreadable DB paths
- [ ] invalid or malformed requests produce structured errors or clear failure behavior without corrupting stdout
- [ ] subprocess integration tests cover handshake, tool listing, and at least one real tool call
- [ ] tests assert that stdout contains only protocol frames
- [ ] failure-path behavior is documented in code/tests clearly enough to prevent regressions

### Testing Requirements

- Unit: targeted error-mapping tests where useful
- Integration: subprocess stdio tests for success and failure paths
- Security: verify diagnostics do not include raw source content or stdout corruption
- Performance: not required

### Dependencies

- Ticket: Implement stdio JSON-RPC framing and MCP request routing
- Ticket: Add MCP tool schemas for all existing CodeAtlas tools

### Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

### References

- `docs/architecture/mcp-server-planning.md`
- `docs/architecture/deployment-modes.md`
- `crates/cli/src/main.rs`

---

### Title

Ticket: Publish supported MCP client setup and troubleshooting docs

### Problem

The end-user goal is simple AI-client setup, but the current docs still frame
MCP as an embeddable library surface. Without concrete client setup examples,
the implementation will still be hard for users to adopt.

### Scope

- update README to present `codeatlas mcp serve --db <path>` as the supported MCP flow once implemented
- add copy-paste setup guidance for a small set of real MCP clients
- include troubleshooting for bad DB paths, startup failures, and validation steps
- align architecture and operations docs with the supported local MCP story
- avoid documenting unsupported hosted or non-stdio modes

### Acceptance Criteria

- [ ] README documents the supported MCP launch flow using `codeatlas mcp serve --db <path>`
- [ ] docs include copy-pasteable setup guidance for Claude Desktop and Cursor
- [ ] docs include one additional ChatGPT/Codex-style local MCP client or wrapper with stable config shape at implementation time
- [ ] docs include a basic troubleshooting section for startup and DB-path failures
- [ ] docs do not overstate support for hosted or non-stdio deployment modes

### Testing Requirements

- Unit: not required
- Integration: manually validate documented config examples against the implemented server where practical
- Security: ensure docs do not recommend unsafe logging or source-sharing practices
- Performance: not required

### Dependencies

- Ticket: Add `codeatlas mcp serve` canonical CLI entrypoint and server wiring
- Ticket: Implement stdio JSON-RPC framing and MCP request routing
- Ticket: Add MCP tool schemas for all existing CodeAtlas tools
- Ticket: Add MCP diagnostics and subprocess integration coverage

### Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing where applicable
- [ ] Docs updated
- [ ] CI green if doc checks exist

### References

- `docs/architecture/mcp-server-planning.md`
- `README.md`
- `docs/architecture/deployment-modes.md`
- `docs/operations/runbook.md`

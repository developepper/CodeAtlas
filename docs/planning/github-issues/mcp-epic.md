## Objective

Make CodeAtlas usable as a simple local stdio MCP server for mainstream AI
clients through the canonical command `codeatlas mcp serve --db <path>`.

## In Scope

- add the canonical CLI entrypoint `codeatlas mcp serve --db <path>`
- implement the minimal stdio MCP subset required for real clients
- expose all existing MCP tools through `tools/list` and `tools/call`
- add clear startup/runtime diagnostics that do not corrupt stdout framing
- add subprocess integration coverage for framed stdio communication
- document copy-paste setup guidance for a small set of real MCP clients

## Out Of Scope

- hosted MCP serving
- HTTP/gRPC APIs
- auth, tenancy, quotas, or billing
- multi-user session management
- dynamic tool registration
- MCP features beyond the minimal tool-serving subset needed for v1
- broader query-surface expansion unrelated to MCP serving

## Child Tickets

- #135 Ticket: Add codeatlas mcp serve canonical CLI entrypoint and server wiring
- #131 Ticket: Implement stdio JSON-RPC framing and MCP request routing
- #133 Ticket: Add MCP tool schemas for all existing CodeAtlas tools
- #132 Ticket: Add MCP diagnostics and subprocess integration coverage
- #134 Ticket: Publish supported MCP client setup and troubleshooting docs
- #136 Ticket: Add MCP packaging and installation path for end users
- #137 Ticket: Validate MCP client compatibility and add minimal interoperability shims

## Epic Definition Of Done

- A user can run `codeatlas mcp serve --db <path>` against a local CodeAtlas
  index.
- A generic stdio MCP client can complete `initialize`, `tools/list`, and
  `tools/call`.
- All existing CodeAtlas MCP tools are exposed with input schemas.
- Server diagnostics never corrupt stdout protocol frames.
- README and supporting docs include copy-paste setup guidance for a small set
  of real MCP clients.
- End-user installation/distribution guidance exists for the canonical MCP flow.
- Documented clients have compatibility validation, and any required
  client-compatibility accommodations are explicit.
- Integration tests cover framed stdio communication with a real subprocess.

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [docs/architecture/deployment-modes.md](docs/architecture/deployment-modes.md)
- [README.md](README.md)
- [crates/server-mcp/src/lib.rs](crates/server-mcp/src/lib.rs)
- [crates/server-mcp/src/registry.rs](crates/server-mcp/src/registry.rs)
- [crates/server-mcp/src/tools.rs](crates/server-mcp/src/tools.rs)
- [crates/cli/src/main.rs](crates/cli/src/main.rs)

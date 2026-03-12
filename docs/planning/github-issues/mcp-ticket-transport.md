## Problem

The current workspace has MCP tool dispatch logic but no stdio transport, no
JSON-RPC framing, and no MCP protocol handler. Real MCP clients cannot talk to
CodeAtlas until that layer exists.

## Scope

- add JSON-RPC request/response/error types
- implement newline-delimited stdio message parsing and serialization
- implement MCP request routing for `initialize`,
  `notifications/initialized`, `tools/list`, and `tools/call`
- delegate `tools/call` to the existing `ToolRegistry`
- guarantee protocol-only stdout output
- handle EOF and malformed request cases cleanly
- reject or fail clearly on unsupported `Content-Length` framed input
- handle SIGTERM/SIGINT shutdown cleanly where practical for local subprocess
  lifecycle management

## Acceptance Criteria

- [ ] the server accepts and emits newline-delimited JSON-RPC messages over stdio
- [ ] `initialize` returns server capabilities and tool-serving support
- [ ] `notifications/initialized` is handled without sending a response
- [ ] `tools/call` delegates to `ToolRegistry::call()`
- [ ] malformed requests return JSON-RPC/MCP-compatible errors without crashing the process
- [ ] stdout contains protocol frames only
- [ ] unsupported `Content-Length` framed input is rejected or fails in a clear, predictable way
- [ ] process shutdown on EOF or termination signals is clean and does not corrupt stdout

## Testing Requirements

- Unit: framing parser/serializer, request parsing, response/error serialization
- Integration: handshake and tool-call flows over stdio against a spawned subprocess, including unsupported `Content-Length` input handling where practical
- Security: verify malformed input does not leak non-protocol output or raw sensitive details
- Performance: basic framing loop should avoid unnecessary buffering or repeated allocations where practical

## Dependencies

- Parent epic: #130
- Depends on #135

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing
- [ ] Docs updated if needed
- [ ] CI green

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [crates/server-mcp/src/registry.rs](crates/server-mcp/src/registry.rs)
- [crates/server-mcp/src/types.rs](crates/server-mcp/src/types.rs)
- [docs/specifications/rust-code-intelligence-platform-spec.md](docs/specifications/rust-code-intelligence-platform-spec.md)

# MCP Client Compatibility Notes

This document records intentional compatibility accommodations in the CodeAtlas
MCP server and their rationale.

## Design Principle

The server targets the generic stdio MCP specification (2025-11-25) and avoids
client-specific branching. Compatibility shims are additive responses to
methods that documented clients probe during startup, not behavioral changes
conditioned on client identity.

## Documented Target Clients

- Claude Desktop
- Cursor
- OpenAI Codex CLI

Any stdio MCP client that speaks newline-delimited JSON-RPC 2.0 and the
supported method subset should work.

## Compatibility Shims

### `ping` method

**Behavior:** Returns an empty JSON-RPC result (`{}`).

**Rationale:** MCP clients may send `ping` as a health check or keepalive. The
MCP specification defines `ping` as a standard method. Returning
`METHOD_NOT_FOUND` can cause clients to treat the server as unhealthy.

**Test coverage:** Unit test `serve_ping_returns_empty_result`, subprocess test
`mcp_stdio_ping_returns_result`.

### `resources/list` method

**Behavior:** Returns `{"resources": []}`.

**Rationale:** Some MCP clients probe `resources/list` during capability
discovery. Returning an empty list communicates "no resources available" without
triggering error-handling paths in clients that expect a structured response.

**Test coverage:** Unit test `serve_resources_list_returns_empty`, subprocess
test `mcp_stdio_resources_list_returns_empty`.

### `prompts/list` method

**Behavior:** Returns `{"prompts": []}`.

**Rationale:** Same as `resources/list`. Clients may probe for prompts during
startup.

**Test coverage:** Unit test `serve_prompts_list_returns_empty`, subprocess
test `mcp_stdio_prompts_list_returns_empty`.

### `notifications/cancelled` notification

**Behavior:** Silently accepted (no response).

**Rationale:** Clients may send this notification when aborting a pending
request. Since it is a notification (no `id`), no response is required per
JSON-RPC. Explicitly accepting it avoids logging noise from the unknown-method
fallback.

**Test coverage:** Unit test `serve_notifications_cancelled_silent`.

### Extra client capabilities in `initialize`

**Behavior:** Ignored. The server returns its own capabilities regardless of
what the client advertises.

**Rationale:** Clients like Cursor advertise capabilities such as `roots` and
`sampling` that the server does not use. The server must not reject these.

**Test coverage:** Unit test `serve_initialize_with_extra_capabilities`,
subprocess test `mcp_stdio_client_handshake_with_extra_capabilities`.

### `tools/list` with cursor parameter

**Behavior:** The `params` field is accepted and ignored. All tools are
returned in a single response.

**Rationale:** Some clients send `{"cursor": null}` when requesting the first
page. Since CodeAtlas has a small fixed tool set, pagination is unnecessary and
cursor values are ignored.

**Test coverage:** Unit test `serve_tools_list_with_cursor_param`.

## Content-Length Framing Rejection

**Behavior:** The server detects lines starting with `Content-Length:` and
returns a `PARSE_ERROR` with a message explaining that the server uses
newline-delimited JSON per MCP spec 2025-11-25.

**Rationale:** The older MCP transport (2024-11-05) used Content-Length framing.
If a client sends this format, the server should fail clearly rather than
silently misbehaving.

**Test coverage:** Unit test `read_message_rejects_content_length`,
`serve_content_length_rejected`, subprocess test
`mcp_stdio_content_length_rejected`.

## What Is Not Shimmed

Methods and features not listed above return `METHOD_NOT_FOUND` per JSON-RPC
2.0. This includes:

- `completions/complete`
- `resources/read`
- `prompts/get`
- `logging/setLevel`
- Any other method not in the supported set

Unknown notifications (method names not matching a known notification) are
silently ignored per JSON-RPC 2.0 convention.

## Adding New Shims

New compatibility shims should only be added when a documented client
demonstrably requires them for basic interoperability. Each shim should:

1. Be minimal and additive (no client-specific branching).
2. Have a `COMPAT:` comment in the source explaining the rationale.
3. Be covered by both a unit test and a subprocess integration test.
4. Be documented in this file.

# Capability Tier Query Behavior

This document describes how query surfaces behave across capability tiers.

## Capability Tiers

Every indexed file is classified into one of these tiers:

| Tier                 | Meaning                                              |
|----------------------|------------------------------------------------------|
| `file_only`          | File record and content only; no extracted symbols   |
| `syntax_only`        | Symbols extracted by a syntax backend (tree-sitter)  |
| `syntax_plus_semantic` | Syntax baseline enriched by a semantic backend     |
| `semantic_only`      | Transitional: semantic backend only, no syntax       |

## Query Surface Behavior by Tier

### File Outline (`get_file_outline`)

| Tier                   | File metadata | Symbols               |
|------------------------|---------------|-----------------------|
| `file_only`            | present       | empty list            |
| `syntax_only`          | present       | syntax-derived        |
| `syntax_plus_semantic` | present       | merged syntax+semantic|

The response includes `capability_tier` on both the file record and each
symbol record. Consumers can use this to understand why a file has no symbols
(file-only tier) versus having syntax-only coverage.

### Symbol Search (`search_symbols`)

Returns symbols from all tiers by default. The MCP `search_symbols` tool
accepts an optional `capability_tier` parameter (one of `file_only`,
`syntax_only`, `syntax_plus_semantic`, `semantic_only`) to restrict results
to a specific tier.

Ranking applies a confidence boost for semantic-tier symbols, reflecting their
higher fidelity. Syntax-only symbols receive no boost. This means
semantic-enriched symbols will rank higher than syntax-only symbols when query
relevance is otherwise equal.

### Exact Symbol Lookup (`get_symbol`, `get_symbols`)

Works identically across all tiers. Every symbol record carries
`capability_tier` and `source_backend` for provenance.

### File Content (`get_file_content`)

Returns file content regardless of tier. The response includes
`capability_tier` on the file metadata so consumers know what level of symbol
coverage is available for that file.

### File Tree (`get_file_tree`)

Each entry includes `capability_tier`, `language`, and `symbol_count`.
Consumers can use this to identify which files in a repo have symbol coverage
and which are file-only.

### Repo Outline (`get_repo_outline`)

File entries include `capability_tier`. The repo-level counts
(`file_count`, `symbol_count`, `language_counts`) reflect all tiers.

### Text Search (`search_text`)

Returns matches from all tiers. Results include the symbol record (with
`capability_tier`) when the match is associated with a symbol.

## MCP Response Format

All file-oriented MCP tool responses include `capability_tier` as a string
field in the appropriate location:

```json
{
  "file": {
    "file_path": "src/lib.rs",
    "language": "rust",
    "symbol_count": 3,
    "capability_tier": "syntax_only"
  }
}
```

File tree and repo outline entries:

```json
{
  "path": "README.md",
  "language": "markdown",
  "symbol_count": 0,
  "capability_tier": "file_only"
}
```

Symbol records always include `capability_tier`:

```json
{
  "name": "greet",
  "kind": "function",
  "capability_tier": "syntax_only",
  "source_backend": "syntax-rust"
}
```

# CodeAtlas

CodeAtlas is a Rust-based polyglot code intelligence platform designed for AI agents and developer tools.

It indexes repositories once, then returns precise symbol- and structure-level answers so tools do not need to read entire codebases for every question.

## What CodeAtlas Is

CodeAtlas is intended to provide:

- Fast symbol search across many programming languages.
- File and repository structural exploration (outlines, trees, symbol maps).
- Semantic-first results where language-native analysis is available.
- Reliable syntax fallback where semantic adapters are unavailable.
- Deterministic outputs suitable for automation and AI tooling.

Primary interface target:

- MCP server (first-class)
- Optional HTTP/gRPC APIs (planned)

## What It Can Do (Target Capabilities)

- `search_symbols`: find relevant functions, classes, methods, types, constants.
- `get_symbol` / `get_symbols`: fetch exact symbol details with locations and signatures.
- `get_file_outline`: retrieve structural map of a file.
- `get_file_tree` / `get_repo_outline`: navigate repository shape quickly.
- `search_text`: fallback full-text retrieval when symbol lookup is insufficient.

## Current Status

CodeAtlas is currently in planning and repository setup.

What exists now:

- Product and implementation specification in `docs/specifications/`.
- Issue-driven execution plan with one-PR-per-issue policy.
- Governance and contribution workflow docs.
- GitHub Actions CI scaffold for PRs and pushes to `master`.

What does not exist yet:

- Rust crates and runtime implementation.
- Production indexing/query engine behavior.
- Released binaries or hosted deployment.

## Design Principles

- Semantic-first, syntax-fallback intelligence.
- Local-first trust model with hosted-ready architecture.
- Security by design (treat indexed code as sensitive, untrusted input).
- Strong determinism and stable API/tool behavior.
- Observability and quality metrics as first-class requirements.

## Roadmap

Planning artifacts:

- `docs/architecture/rust-code-intelligence-plan.md`
- `docs/planning/issue-backlog.md`
- `docs/workflow/github-process.md`
- `docs/engineering-principles.md`

Canonical specification:

- `docs/specifications/rust-code-intelligence-platform-spec.md`

## Contributing

Contributions follow strict quality gates and issue/PR discipline.

Start here:

- `CONTRIBUTING.md`

## License

License is not defined yet.

Add a `LICENSE` file before first public release.

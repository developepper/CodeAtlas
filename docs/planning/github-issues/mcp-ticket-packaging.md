## Problem

Even with a correct MCP server implementation, end-user adoption will remain
weaker than it should be if installation and launch are not treated as part of
the feature. Users need a straightforward way to obtain and run `codeatlas`
for MCP usage without inventing their own packaging story.

## Scope

- define the supported distribution path for MCP-capable `codeatlas` builds
- document the canonical installation story for end users
- ensure the install/run story aligns with `codeatlas mcp serve --db <path>`
- add any packaging or release notes needed so MCP usage is a first-class part
  of distribution rather than an implied local build-only workflow
- avoid adding a separate product-facing server binary unless required
- treat `cargo install` and GitHub Release binaries as the realistic v1 paths,
  with package-manager integrations such as Homebrew explicitly deferred unless
  they become necessary

## Acceptance Criteria

- [ ] the project has a documented supported install path for end users who want to use MCP
- [ ] packaging/release docs make `codeatlas mcp serve --db <path>` discoverable as the MCP launch path
- [ ] the chosen install story does not require users to build custom wrappers
- [ ] any release/distribution updates needed for MCP support are captured in repo docs or release process notes
- [ ] the packaging guidance remains consistent with the product decision to keep `codeatlas` as the canonical executable
- [ ] the initial supported packaging story is explicit about what is in v1 (`cargo install` and/or GitHub Release binaries) and what is deferred

## Testing Requirements

- Unit: not required
- Integration: validate the documented install/run flow on a clean environment where practical
- Security: ensure install guidance does not encourage unsafe execution or broad permissions
- Performance: not required

## Dependencies

- Parent epic: #130
- Depends on #135

## Definition Of Done

- [ ] Acceptance criteria met
- [ ] Tests added/updated and passing where applicable
- [ ] Docs updated
- [ ] CI green if affected

## References

- [docs/architecture/mcp-server-planning.md](docs/architecture/mcp-server-planning.md)
- [README.md](README.md)
- [docs/planning/post-v1-roadmap.md](docs/planning/post-v1-roadmap.md)

# AI Ticket Workflow

This document defines the standard implementation and review loop for
AI-assisted ticket work in CodeAtlas.

The goal is simple:

- one ticket at a time
- one implementation session at a time
- one independent review session at a time
- repeat until acceptance criteria are actually met

This workflow is intended to reduce drift, make reviews stricter, and keep
implementation aligned with the project's architecture and engineering
principles.

## Core Rules

- Work one primary ticket at a time.
- Follow the ticket acceptance criteria exactly.
- Treat the ticket, architecture docs, and engineering principles as required
  inputs, not optional context.
- Prefer the correct long-term architecture over backward compatibility when
  the two conflict.
- Do not silently expand scope. If the work exceeds the ticket, open a
  follow-up issue instead.
- Do not close or mark a ticket complete until the review loop returns no
  findings and acceptance is explicitly checked.

## Required Context For Every Ticket Session

Every implementation or review session must load:

- the ticket issue body or ticket draft
- `docs/architecture/universal-syntax-indexing-architecture.md` when working
  within Epic 17 or any follow-on work that depends on its architecture
- `docs/planning/universal-syntax-indexing.md` when working within Epic 17
- `docs/engineering-principles.md`
- any additional references listed in the ticket itself

Fresh sessions should be able to operate correctly from those artifacts alone.

## Roles

### Implementation Session

Responsibilities:

- understand the ticket and its dependencies
- make the required code and doc changes
- run the relevant tests
- verify acceptance criteria one by one
- summarize what remains incomplete, if anything

Implementation session output should include:

- what changed
- which acceptance criteria are met
- which tests were run
- any residual risks or open questions

### Review Session

Responsibilities:

- review the current diff or working tree against the ticket
- review against architecture and engineering-principles constraints
- prioritize bugs, regressions, architecture violations, missing tests, and
  unmet acceptance criteria
- return findings first, ordered by severity

Review session output should:

- list findings first
- include file references
- state explicitly when there are no findings
- mention residual testing gaps even when no findings remain

The review session is not there to restate the implementation or to be
supportive. It is there to decide whether the ticket is actually ready.

### Acceptance-Criteria Gaps

Review sessions are also allowed to conclude that the ticket itself is
insufficient.

Examples:

- acceptance criteria miss an important behavioral contract
- acceptance criteria allow an implementation that conflicts with the
  architecture doc or engineering principles
- acceptance criteria omit a necessary regression test or observable behavior

When that happens, the review should report it as a finding rather than saying
"no findings" simply because the current checklist was satisfied.

Required handling:

- flag the acceptance-criteria gap explicitly
- explain why the current ticket contract is insufficient
- escalate to the human gate for a decision on whether the ticket should be
  edited, split, or followed by a new issue

Do not treat a ticket as complete when the implementation is correct only
against an incomplete or misleading ticket definition.

## Standard Loop

1. Start from one ticket with explicit dependencies satisfied.
2. Run an implementation session against that ticket.
3. Run an independent review session against the resulting changes.
4. If review findings exist, return to implementation and fix them.
5. Repeat implementation and review until review returns no findings.
6. Confirm acceptance criteria and required tests are complete.
7. Only then prepare the PR or merge path.

## Acceptance Gate

A ticket is ready only when all of the following are true:

- every acceptance criterion is satisfied
- the acceptance criteria themselves are still judged sufficient by review
- required tests are added or updated
- required tests pass
- docs are updated where needed
- the review session returns no findings
- the implementation still aligns with the canonical architecture for that
  area

If any of those are not true, the ticket is not done.

## Dependency Discipline

- Do not begin a ticket whose required design or implementation dependencies
  are not met.
- For Epic 17 specifically, Ticket 1 is the design gate.
- Tickets that depend on real syntax-indexed behavior should not proceed before
  the platform and at least the required language implementations exist.

If a dependency is unclear, resolve it before implementation starts.

## Suggested Prompt Shape

### Implementation Prompt

Use a prompt with this structure:

```text
Work Ticket <id> to completion.

Required context:
- <ticket path or issue body>
- docs/engineering-principles.md
- additional architecture/planning docs required by the ticket

Rules:
- follow the acceptance criteria exactly
- prefer the correct long-term architecture over backward compatibility
- do not commit unless explicitly asked
- run the relevant tests
- summarize acceptance status, test evidence, and residual risks
```

### Review Prompt

Use a prompt with this structure:

```text
Review the current changes for Ticket <id>.

Review against:
- <ticket path or issue body>
- docs/engineering-principles.md
- additional architecture/planning docs required by the ticket

Prioritize:
- correctness
- regressions
- architecture alignment
- missing tests
- unmet acceptance criteria

Return findings first, ordered by severity, with file references.
If there are no findings, say so explicitly and mention residual risks or
testing gaps.
```

## Branch And PR Expectations

This document supplements `docs/workflow/github-process.md`.

Use:

- `ticket/<issue-id>-<slug>` branches for normal implementation
- one PR per ticket
- the PR template
- explicit test evidence in the PR body

Do not combine multiple tickets into one implementation PR unless the issue
structure itself is changed first.

## Epic 17 Notes

For Epic 17, the default execution order is:

1. `#174`
2. `#175`
3. `#176`
4. `#177`
5. `#178` / `#179` / `#180` / `#181`
6. `#182`
7. `#183`

Important Epic 17 rule:

- `#174` is the architecture gate
- `#182` should not begin before `#176` plus at least one language ticket
  provide real syntax-indexed behavior
- `#183` should reflect real implementation evidence, not only planned intent

## Human Gate

AI sessions can implement and review, but a human should remain the final gate
for:

- tradeoff decisions where multiple acceptable answers exist
- confirming that acceptance criteria are truly satisfied
- deciding whether a follow-up issue is needed instead of scope expansion
- approving merge readiness

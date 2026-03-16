# Universal Syntax Issue Creation Order

Recommended GitHub issue creation order for Epic 17.

## Create First

1. Epic 17: Universal syntax indexing platform
2. Ticket 1: Define the universal syntax indexing architecture and capability model

Rationale:

- Ticket 1 is the explicit design gate for the rest of the epic.
- Tickets 2-10 can be created immediately after Ticket 1 exists, but they
  should reference Ticket 1 as a dependency and should not proceed on
  conflicting design assumptions before Ticket 1 is merged or otherwise
  ratified.

## Create Next

3. Ticket 2: Refactor core model and metrics for file/syntax/semantic capability tiers
4. Ticket 3: Create the multi-language syntax indexing subsystem and migrate Rust onto it

Rationale:

- Ticket 2 and Ticket 3 define the first real implementation boundary.
- Ticket 3 is the enabling substrate for all new language syntax tickets.
- Ticket 2 may need to land before or alongside Ticket 3 depending on the final
  schema/model changes required by the new subsystem.

## Create After Platform Exists

5. Ticket 4: Implement PHP syntax indexing on the new subsystem
6. Ticket 5: Implement Python syntax indexing
7. Ticket 6: Implement Go syntax indexing
8. Ticket 7: Implement Java syntax indexing
9. Ticket 8: Implement JavaScript syntax indexing

Rationale:

- PHP is the first proving ground and should be prioritized immediately after
  the platform exists.
- The remaining language tickets can be sequenced by interest, value, or
  implementation complexity once the platform is stable.

## Create After At Least One New Syntax Language Lands

10. Ticket 9: Rework query surfaces for broad syntax coverage

Rationale:

- Query-surface work should validate real syntax-indexed repositories, not only
  the abstract architecture.
- Ticket 9 depends most meaningfully on Ticket 3 plus at least one language
  implementation, ideally PHP.

## Create Near Epic Closeout

11. Ticket 10: Update benchmark and token-efficiency evaluation strategy

Rationale:

- Benchmark and documentation updates should reflect real implementation and
  proving-ground evidence rather than only planned architecture.
- This ticket is still worth creating early for visibility, but it should land
  after the first substantive implementation evidence exists.

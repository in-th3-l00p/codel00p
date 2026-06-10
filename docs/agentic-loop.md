# Agentic Loop

codel00p uses a bounded repeatable loop for long-running agentic development.
The goal is continuous progress toward the finished product without losing
engineering discipline, test coverage, or clean history.

This is not an uncontrolled background process. Each cycle is a complete,
reviewable unit of work that starts from a clean tree and ends with verified,
committed, pushed code.

## Loop Contract

Every cycle follows the same order:

1. Confirm `main` is clean and synced.
2. Read the current roadmap and backlog.
3. Select the highest-leverage next slice.
4. Write or update a focused implementation plan.
5. Implement test-first.
6. Run targeted verification.
7. Run `pnpm verify`.
8. Commit without coauthor trailers.
9. Push `main`.
10. Update docs or status if the shipped behavior changes the product surface.
11. Start the next cycle.

## Hard Gates

- Do not start a new cycle with uncommitted changes.
- Do not mix unrelated product slices in one commit.
- Do not claim completion without fresh verification evidence.
- Do not bypass tests for core behavior.
- Do not revert user changes unless explicitly instructed.
- Do not add coauthor trailers.
- Do not turn roadmap items into implementation until the target slice is
  narrow enough to test and ship independently.

## Priority Order

When multiple slices are available, choose in this order:

1. Work that makes the CLI/harness more production-useful today.
2. Work that strengthens project memory quality and review.
3. Work that improves provider breadth, routing, policy, and auditability.
4. Work that certifies external MCP interoperability.
5. Work that prepares the cloud, desktop, and team-control surfaces.
6. Documentation that removes ambiguity for contributors or future agents.

## Cycle Output

Every cycle should leave behind:

- tests for the shipped behavior;
- a focused commit;
- a pushed `main`;
- updated docs when product status changed;
- a clear next recommended slice.

## Stop Conditions

Pause and ask for direction only when:

- the same blocker repeats after real debugging attempts;
- the next step requires a product decision not encoded in the docs;
- verification fails for a reason outside the current slice;
- credentials, external services, or user approval are required;
- the finished-product roadmap has no remaining actionable work.

# Project Memory

Project memory is the central differentiator of codel00p.

The goal is not to store every chat, prompt, response, or terminal output. The
goal is to preserve useful knowledge in a compact, reviewed, reusable form.

## What memory should capture

Project memory should capture knowledge such as:

- repository structure and module responsibilities;
- important paths and entry points;
- architecture decisions and rationale;
- rejected alternatives;
- setup, test, deployment, and rollback workflows;
- debugging procedures;
- recurring errors and fixes;
- team conventions;
- review preferences;
- product and domain terminology;
- summaries of important completed tasks.

## Memory categories

Recommended categories:

- **Codebase facts:** stable facts about files, modules, services, and ownership.
- **Architecture decisions:** decisions, rationale, consequences, and tradeoffs.
- **Workflows:** repeated operational procedures.
- **Team conventions:** style, review, naming, and process preferences.
- **Task outcomes:** useful summaries from completed work.
- **Domain glossary:** business and product language.

## Lifecycle

Memory should move through a deliberate lifecycle:

1. **Observe:** agent sessions and developer work produce candidate knowledge.
2. **Extract:** codel00p converts useful context into memory candidates.
3. **Review:** a human approves, edits, scopes, or rejects each candidate.
4. **Store:** approved memory is saved locally.
5. **Sync:** approved memory can be shared with an organization.
6. **Retrieve:** future sessions load relevant memory for the current task.
7. **Refine:** stale or low-value memory is corrected, merged, archived, or
   deleted.

Review is required because unreviewed memory becomes noise quickly.

## Local-first storage

The first memory implementation should work locally. It should not require a
cloud account, hosted database, or organization workspace.

Local storage should prioritize:

- readability;
- portability;
- easy review;
- deterministic retrieval;
- simple backup and versioning.

Cloud sync can come later once the local memory loop proves useful.

## Retrieval principles

Memory retrieval should be:

- relevant to the current project;
- relevant to the current task;
- scoped by file, module, workflow, or team context where possible;
- compact enough to fit into agent context;
- transparent enough that users can inspect what was loaded.

The user should be able to understand why memory was used and correct memory
that is stale or wrong.

## Quality bar

Good memory is:

- short;
- specific;
- reviewed;
- source-aware;
- easy to update;
- useful across future sessions.

Bad memory is:

- vague;
- duplicated;
- stale;
- copied directly from raw chat;
- too long to retrieve often;
- impossible to verify.

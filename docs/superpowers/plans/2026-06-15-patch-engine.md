# Slice: resilient patch engine for `apply_patch`

Date: 2026-06-15
Roadmap: Milestone 1 (Local Agent Foundation) — "reliable file editing".
Priority order #1: edit reliability is the top lever for agentic coding quality.

## Goal

Make the agent's `apply_patch` edits as reliable as (or better than) Claude
Code's, so whitespace, indentation, and line-ending drift between what the model
sends and what the file actually contains no longer cause spurious edit
failures — without ever corrupting bytes outside the matched region.

## Why this slice

Exact-string matching is brittle and is the single biggest cause of
coding-agent edit failures: the model reproduces a code block from memory and
gets the trailing spaces, the indentation width, or the CRLF/LF endings subtly
wrong, and the edit hard-fails. The fix is a tolerant matching engine with
uniqueness safety and actionable errors, kept fully backward compatible in tool
name and existing schema.

## Design

All changes live in `editing.rs` (+ its tests). The tool name (`apply_patch`),
its `changes` array schema (`path`/`find`/`replace`), and
`PermissionScope::WorkspaceWrite` are unchanged. New, additive surface:

- An optional `replace_all: bool` per change (defaults to `false`).
- An additive `strategy` field in each per-change result summary.

`apply_change(original, find, replace, replace_all)` drives an ordered list of
strategies; the **first** strategy that yields ≥1 match wins:

1. **`exact`** — substring fast path (preserves prior behaviour).
2. **`line-ending`** — line-window match after normalising CRLF→LF on both
   sides, so an LF `find` edits a CRLF file (and vice versa).
3. **`trailing-whitespace`** — line-window match ignoring trailing whitespace
   per line.
4. **`indentation`** — line-window match ignoring a uniform leading-whitespace
   shift, then **re-indent the replacement** to the file's actual indentation so
   surrounding formatting is preserved.

Each strategy returns located byte ranges; `splice` rebuilds the file by copying
every byte outside the matched ranges verbatim, guaranteeing no collateral
corruption. Line windows keep their newlines so offsets reconstruct exactly.

**Uniqueness safety**: if a strategy finds >1 match and `replace_all` is not
set, it errors with the exact count and tells the model to add context or set
`replace_all`. With `replace_all`, every occurrence is replaced.

**Actionable not-found errors**: `not_found_hint` reports when a line differs
only in whitespace, otherwise surfaces the closest near-miss line, so the model
can self-correct.

## Tests (in `tests/editing_tools.rs`)

- exact replacement (existing test, now asserts `strategy: "exact"`);
- trailing-whitespace drift across a multi-line find;
- indentation drift that preserves the file's real (wider) indent;
- CRLF file edited with an LF find, other CRLF lines left intact;
- multi-occurrence rejection (count + `replace_all` hint, no write);
- `replace_all` replacing every occurrence;
- not-found error text hinting at a whitespace-only difference;
- a real multi-line Rust edit asserting all surrounding bytes are preserved.

## Out of scope (follow-ups)

Fuzzy/similarity matching (e.g. Levenshtein), unified-diff or hunk-based patch
input, multi-line internal-whitespace collapsing, and applying the same
tolerant engine to a future line-range edit tool.

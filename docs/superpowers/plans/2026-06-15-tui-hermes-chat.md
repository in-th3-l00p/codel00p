# Slice: Hermes-inspired chat polish (markdown + personality)

Date: 2026-06-15
Roadmap: Milestone 6 / TUI initiative — bring the chat closer to the Hermes
reference (`ui-tui/`).

## Context

After studying the Hermes agent's React+Ink chat TUI (`ui-tui/src/components/`),
the codel00p ratatui chat already matched its structure (scroll, composer, role
gutters) but lacked two of Hermes's most visible touches: **Markdown-rendered
assistant messages** (Hermes' `markdown.tsx`/`streamingMarkdown.tsx`) and the
**personality layer** (rotating "verbs" in the spinner, composer placeholders —
`content/verbs.ts`, `placeholders.ts`). This slice adds those, keeping codel00p's
own theme/identity rather than copying Hermes' gold branding.

## What shipped

- **Markdown renderer** (`tui/markdown.rs`, new): assistant message bodies render
  as Markdown — fenced code blocks (with a language label, indented, code-colored),
  ATX headings, horizontal rules, blockquotes (`│` gutter), unordered/ordered lists
  (`•`/`N.` with hanging indent), and inline **bold** / *italic* / `` `code` ``.
  Output is pre-wrapped span-by-span so it integrates with the transcript's exact
  scroll math. `assistant_block` (`view.rs`) keeps the bold `codel00p` role header
  and feeds the body through the renderer.
- **Personality** (`tui/flavor.rs`, new): the status spinner shows a rotating
  thinking verb (`thinking`, `pondering`, …) that switches to a tool-specific verb
  while a tool runs (`reading`, `running`, `searching`, …), plus a long-run "charm"
  after a few seconds. The empty composer shows a rotating placeholder hint seeded
  by the session id. The empty transcript shows a compact welcome banner.

## Tests

- `markdown.rs`: bullets/headings, fenced code + language label, inline
  bold/italic/code as distinct styled spans, paragraph wrapping, blockquote + hr.
- `flavor.rs`: tool-verb mapping + fallback, tick-rotating thinking verb, charm
  timing, stable seeded placeholder.
- `view.rs` `TestBackend`: assistant message renders markdown bullets in the
  transcript; the empty transcript shows the welcome banner.

## Verification

`cargo fmt --all -- --check`, `cargo test --workspace` (103 suites green),
`cargo clippy --workspace --all-targets -- -D warnings`.

## Out of scope (follow-ups, all present in Hermes)

- Syntax highlighting inside code blocks; tables; math; task lists; links.
- Streaming-aware markdown (stable-prefix/unstable-suffix split) — today the whole
  assistant block re-renders each token, which is fine at chat sizes.
- Kaomoji faces, the gold/bronze theme, the ASCII banner art, and the rich status
  segments (context bar, cost, session HUD).

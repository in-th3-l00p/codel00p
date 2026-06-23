# Module Conventions

[Architecture](architecture.md) defines the crate boundaries (what each crate
owns). This document defines the layer *below* that: how code is organized
*within* a crate, so each file is readable on its own and the whole stays easy to
navigate and extend.

These are conventions, not laws — but a change that breaks one should say why.

## Principles

1. **One module, one responsibility.** A file should have a single reason to
   change. When a file mixes two concerns (an algorithm and the I/O around it, a
   data type and its five consumers), split along that seam.

2. **Separate pure logic from I/O.** Deterministic, side-effect-free logic (a
   parser, a matcher, a formatter, a classifier) belongs in its own module with
   no knowledge of the workspace, network, or clock. This is the highest-leverage
   split: pure modules are trivially unit-tested in isolation, and the I/O layer
   above them becomes thin orchestration.

3. **Thin module roots, focused submodules.** Use the `foo.rs` + `foo/` layout
   (not `foo/mod.rs`). The root file declares the submodules and re-exports the
   public surface; the substance lives in the submodules. The rest of the crate
   keeps importing from `crate::foo`, so a split is invisible to callers.

4. **Document the *why*, not the *what*.** Every module opens with a `//!`
   comment stating its job and how it fits its neighbours. Non-obvious items get a
   `///` explaining the reasoning a reader can't recover from the code. Skip
   comments that restate the signature.

5. **Tests live with the code they cover.** Pure modules carry inline
   `#[cfg(test)] mod tests` exercising their logic directly. Behaviour that spans
   modules (a tool against a real workspace, a turn through the harness) is
   covered by integration tests under `tests/`.

6. **Consistent, actionable errors.** Shared error/guard helpers live in one
   place per area (e.g. `editing::support`) so wording and boundary checks stay
   uniform and a failure tells the caller how to recover.

7. **Size is a smell, not a limit.** There is no hard line count, but a file past
   ~500 lines, or one you have to scroll to hold in your head, is usually two
   modules wearing a trench coat. Split it.

## Worked example: `editing`

`codel00p-harness/src/editing.rs` was a single 900-line file holding six tool
implementations, their shared helpers, and the entire patch-matching algorithm.
It now follows the layout above:

```text
editing.rs            module root: declares submodules, re-exports the tools
editing/patch.rs      the pure find/replace engine — no I/O, unit-tested inline
editing/apply_patch.rs the apply_patch tool: schema, normalization, atomic batch
editing/file_ops.rs   whole-file tools: create/update/delete/move/copy
editing/support.rs    shared file-existence and destination guards
```

The win is `patch.rs`: a self-contained string algorithm that now has its own
unit tests and that `apply_patch.rs` consumes as orchestration. Each file states
its job in one sentence and can be read without the others.

## Refactor roadmap

The same treatment applies to the crate's other oversized modules. In rough
priority order (largest / most-mixed first), candidates and their natural seams:

| File | Seam to split on |
| --- | --- |
| `cli/src/tui/update.rs` (~2200) | one submodule per message/update group |
| `cli/src/tui/view.rs` (~1650) | one submodule per screen/panel renderer |
| `harness/src/terminal/{ssh,docker}.rs` (~1500/1400) | connection vs command-exec vs file-transfer |
| `harness/src/code_exec.rs` (~1080) | language/runtime detection vs execution vs result shaping |
| `harness/src/capability.rs` (~930) | capability model vs propose/verify/load flow |
| `harness/src/agent/turn.rs` (~900) | already partly split (`turn/`); continue extracting phases |

Tackle these one module at a time, keeping the test suite green at each step — a
refactor that changes behaviour is a different change and needs its own review.

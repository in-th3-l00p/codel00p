# Slice: Skills dialog — reversible disable, confirm + help overlay

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands" (skills dialog polish).

## Goal

Bring `codel00p skills` to a state-of-the-art finish: make disabling a skill no
longer a dead end (you can see and restore disabled skills without leaving the
dialog), guard the destructive `d` with a confirm step, and add a `?` help
overlay. Work stays in `skills_ui/` + the `codel00p_skill` engine; the
scriptable `skills` subcommands remain byte-identical.

## Design

- **Engine (`codel00p-skill`).** `archive_skill` / `restore_skill` already
  existed (the archive lives under `<root>/.archive`), but there was no way to
  *list* archived skills. Added one small function, `load_archived(skills_dir)`,
  mirroring `load_candidates` (a `scan_dir` over `archive_root`). Archived
  skills are still never loaded as active (verified in the new unit test).
- **Model (`skills_ui/model.rs`, store-free).**
  - New `SkillKind::Disabled` and `Filter::Disabled` (the Tab cycle is now
    Active → Candidates → Disabled → All).
  - New `Mutation::Restore { name, root }`.
  - `u` / `e` restore a disabled skill (immediate, like approve/restore).
  - `d` no longer mutates immediately: it arms `pending_disable: Option<(name,
    root)>` and sets a status prompt. The next key is intercepted — `y`
    confirms (emits `Mutation::Disable`), any other key cancels.
  - `show_help: bool`; `?` toggles it. While shown, the very first key (incl.
    Esc) closes it and is otherwise swallowed.
- **Driver (`skills_ui/mod.rs`).** `load_rows` now also appends `disabled_row`s
  from `load_archived` per source; `apply` handles `Restore` via `restore_skill`.
- **View (`skills_ui/view.rs`).** Footers mention restore + `? help`; a new
  centered `?` overlay via `Clear` + `dialog::panel("help")` + a `Paragraph` of
  `key — description` lines (accent keys, muted descriptions) listing every key.

## Tests

Pure model tests: disable-confirm applies on `y` and cancels on any other key;
restore targets a disabled skill (and is a no-op-with-hint elsewhere); help
toggles and the first key closes it (and swallows a would-be `d`); Tab now walks
through Disabled. Engine: `load_archived` lists archived skills and clears after
restore. `cargo fmt --all -- --check`, `cargo test -p codel00p-cli`,
`cargo test -p codel00p-skill`, and `clippy -D warnings` on both crates are
green.

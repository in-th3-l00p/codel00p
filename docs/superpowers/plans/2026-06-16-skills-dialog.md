# Slice: Skills review dialog (CLI simplification, Phase 3b)

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands".

## Goal

Make `codel00p skills` (bare, on a TTY) open a review dialog: a filterable list
of active skills and agent-proposed candidates, Enter to read a skill's
SKILL.md, then approve/reject candidates or disable an active skill. The
scriptable `list`/`show`/`create`/`candidates`/`approve`/`reject`/`curate`
subcommands stay byte-identical for non-TTY use.

## Design

- **`src/skills_ui/`** (on `crate::dialog`): `model.rs` (pure
  `Picker<SkillRow>` + `Filter { Active, Candidates, All }` + `Screen { List,
  Detail }` + `update(KeyEvent) -> Flow { Stay, OpenDetail, Mutate, Quit }`,
  store-free — every row carries its skills-root `PathBuf` so a mutation targets
  the right directory), `view.rs` (tabbed list + scrollable metadata/instructions
  detail), `mod.rs` (driver: builds rows from `load_skills` + `load_candidates`
  per source via the existing `skills::{skill_sources, user_skills_dir}`, and on a
  `Mutate` calls `approve_candidate`/`reject_candidate`/`archive_skill`, then
  reloads). The whole `run` is ~15 lines thanks to `dialog::run_blocking`.
- **Actions.** `a` approve / `r` reject apply to candidates; `d` disable
  (reversible `archive_skill`) applies to active skills. Pressing an action key
  on a row of the wrong kind is a no-op with a hint in the status line. The
  store has no enable/disable-by-flag API, so "disable" reuses the existing
  reversible archive path (same as `curate`); restore stays a follow-up.
- **`skills.rs`** — bare invocation opens the dialog when stdin+stdout are
  terminals (`std::io::IsTerminal`); non-TTY keeps the existing default (`list`).
  Reuses the same skill APIs the subcommands call — no skill logic reimplemented.
- **`main.rs`** — `mod skills_ui;` registered (one line).
- **`help.rs`** — `SKILLS_HELP` leads with the dialog; subcommands move under a
  "For scripting" heading. No `help_cli` assertion covers skills, so no test
  change was needed there.

## Tests

10 pure `skills_ui::model` update tests: Tab cycles the filter without a store
reload; the filter hides rows of the other kind; Enter → OpenDetail; approve from
list/detail targets a candidate; reject from detail; disable targets an active
skill; approve-on-active and disable-on-candidate are hinted no-ops; Esc from
detail returns to list; Esc from list quits. `cargo test -p codel00p-cli`,
`fmt --check`, `clippy -D warnings` all green.

## Next (Phase 3)

`cloud` (reuse the entity browser), `cron` (list + enable/disable/run/delete),
plus the deferred in-dialog restore for disabled skills and merge/restore for
memory.

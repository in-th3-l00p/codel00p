# codel00p-e2e

A black-box, **fully hermetic** end-to-end test harness that drives the real
`codel00p` binary as a subprocess against a scripted mock provider. No network,
no API keys, no shared global state — every run gets its own tempdir
`CODEL00P_HOME`, its own workspace directory, and its own `memory.sqlite`.

This is the per-feature e2e matrix. The harness API quality is the point; the
pilot proves it end to end and the [coverage matrix](#coverage-matrix) maps every
agent feature to the scenarios that exercise it.

## Running

All cargo commands run from the workspace root (`core/`):

```bash
# the pilot + smoke tests
cargo test -p codel00p-e2e

# interactive exploration: launch the binary against a canned mock and stream events
cargo run -p codel00p-e2e --example repl -- "Create and commit notes.txt"
```

`cargo test -p codel00p-e2e` is self-contained: if the `codel00p` binary has not
been built yet, the harness builds it on demand the first time it is needed (see
binary resolution below).

## Coverage matrix

Every row drives the real `codel00p` binary through the mock provider unless
marked **[live]**. Each scenario file lives under `tests/`. "Hermetic" means it
runs in the default CI e2e job with no secrets; **[live]** tests are `#[ignore]`d
and documented in-file, to be run manually with provider keys and
`CODEL00P_INTEGRATION_TESTS=1`.

| Feature group | File | What it proves | Status |
|---|---|---|---|
| Pilot (full stack) | `pilot.rs` | One scenario through turn loop → 3 tool-set presets → permissions → persistence → memory extraction → session replay | Hermetic |
| Harness smoke | `smoke.rs` | Cross-crate binary resolution + on-demand build | Hermetic |
| Read-only tools | `tools_readonly.rs` | `read_file`/`list_files`/`search_text`/`find_files`/`grep`/`repo_map` | Hermetic |
| Editing tools | `tools_editing.rs` | `create_file`/`update_file`/`delete_file`/`apply_patch` incl. multi-edit atomicity | Hermetic |
| Command tools | `tools_command.rs` | `run_command` foreground + background (`process_*`), timeout/cap | Hermetic |
| Git tools | `tools_git.rs` | `git_status`/`diff`/`log`/`commit`/`prepare_pr` | Hermetic |
| Planning + web | `planning_web.rs` | `update_plan`; `web_fetch` + `web_search` vs mock HTTP; real Brave path | Hermetic + 1 **[live]** |
| Turn control | `turn_control.rs` | `tool_choice`, `response_format`, multi-iteration loop, result truncation, max-iteration cap, event ordering | Hermetic |
| Context assembly | `context.rs` | CODEL00P.md/AGENTS.md/CLAUDE.md loading, approved-memory injection, skill injection, ContextManifest determinism | Hermetic |
| Memory loop | `memory_loop.rs` | extract → review → inject lifecycle through CLI/store | Hermetic (+ gated cases) |
| Delegation | `delegation.rs` | `delegate_task` shared + worktree isolation (parent tree untouched) | Hermetic |
| Pipeline + capability | `pipeline_capability.rs` | `run_pipeline`; propose/verify → approved `CapabilityTool` execution | Hermetic |
| MCP | `mcp.rs` | stdio server tool round-trip; progressive disclosure (`tool_search`/`tool_describe`) past the 15-tool threshold | Hermetic |
| Sessions | `sessions.rs` | run/resume/continue/list recency/replay/sub-agent lineage | Hermetic |
| Providers | `providers.rs` | retry/backoff (429/503 same-route), model catalog | Hermetic (+ gaps below) |
| CLI surfaces | `cli_surfaces.rs` | version/help/config/providers/skills/cron(add/list/run)/cloud(status/push/pull vs mock)/update | Hermetic |
| Permissions | `permissions.rs` | allow/ask/deny modes, remembered decisions, denied-tool events | Hermetic |
| Nav robustness | `nav_robustness.rs` | regression for the `list_files`/`search_text` ignore-aware + unreadable-dir-tolerant walk (PR #61) | Hermetic |
| Default tool set | `default_tool_set.rs` | regression: `agent run` with no `--tool-set` advertises the editing tools and can actually create a file | Hermetic |

### Known gaps (documented, not faked)

These were found while building the matrix and are asserted as the *real* current
behavior (or `#[ignore]`d with an explanation) rather than papered over:

- **Provider fallback routing** is a `codel00p-providers` request-builder API
  (`InferenceRequest::fallback_route_with_base_url`) with **no CLI flag / config
  key / env var** — not reachable through the binary. Library coverage lives in
  `codel00p-providers/tests/fallback_routing.rs`.
- **Usage / cost** is parsed at the providers layer but the `AgentEvent` protocol
  carries no usage/cost field and the CLI prints none, so e2e asserts its
  *absence*. Library coverage: `codel00p-providers/tests/usage_cost.rs`.
- **Working-plan contents** live in an in-memory `PlanStore` with no dedicated
  `AgentEvent`; the only end-to-end observable is the echoed `update_plan` tool
  result in the next request, which is what `planning_web.rs` asserts.
- **`web_search` live** (real Brave endpoint), **`update --check` / bare
  `update`** (hard-coded GitHub release URL), and one **fallback** note are
  `#[ignore]`d [live]/manual-only and documented in-file.

## How it works

### `CodelRunner` — the isolated world

```rust
use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::json;

let runner = CodelRunner::new()
    .workspace_file("README.md", "# pilot\n") // seed files
    .git_init();                              // git init -b main + identity + initial commit

let provider = MockProvider::start()
    .tool_call("create_file", json!({ "path": "pilot.txt", "content": "hi\n" }))
    .tool_call("run_command", json!({ "program": "git", "args": ["add", "pilot.txt"] }))
    .tool_call("git_commit", json!({ "message": "feat: add pilot.txt" }))
    .assistant_text("Done.\nremember convention[pilot]: pilot.txt holds the greeting.");

let runner = runner.with_provider(&provider);
let result = runner.run(&["agent", "run", "Create and commit pilot.txt.", "--tool-set", "all"]);
result.assert_success();
```

- `CodelRunner::new()` allocates a tempdir `CODEL00P_HOME` and a tempdir
  workspace.
- `.git_init()` runs `git init -b main` (always `-b main`: CI git defaults to
  `master`), sets a deterministic `user.name`/`user.email`, and makes an initial
  commit so the tree starts clean.
- `.workspace_file(path, contents)` seeds files. **Seed before `git_init()`** if
  you need the file tracked in the initial commit (the `git_commit` tool refuses
  to run while untracked files are present).
- `.with_provider(&mock)` wires the mock's base URL. For `agent …` subcommands
  the runner then injects `--provider custom --model test-model --base-url <url>
  --json-events --permission-mode allow --workspace <ws>` automatically.
- `.run(args)` injects `CODEL00P_HOME`, `--memory-db`, the org/project flags, and
  `CODEL00P_PROVIDER_CUSTOM_API_KEY=test-token`, then spawns the binary and
  returns a `RunResult`. **Reusing the same runner** for a second `.run(...)`
  (e.g. `["session", "show", id]`) reuses the same home — that is how replay
  scenarios reconstruct prior state.

### `MockProvider` — the scripted conversation

`MockProvider` wraps an `httpmock` server that answers `POST /chat/completions`
with a pre-scripted, **ordered** sequence of OpenAI chat-completions responses:

- `.assistant_text("…")` — a final assistant message (`finish_reason: "stop"`).
- `.tool_call(name, json_args)` — an assistant `tool_calls` turn
  (`finish_reason: "tool_calls"`); `json_args` is JSON-encoded into the OpenAI
  `arguments` string, matching what the `custom` transport parses.
- `.tool_calls(vec![...])` — several tool calls in one turn.
- `.base_url()`, `.hits()`, `.received_requests()` for wiring and assertions.

**Ordering mechanism.** The agent re-calls the model after each tool result, so
every iteration's request body carries one *more* `"role":"tool"` message than
the last. Each queued turn is registered with a custom matcher
(`when.is_true(...)`) that fires only when the request contains exactly its index
worth of tool results. This serves turns deterministically and idempotently
without relying on mutable call counters inside matchers (which httpmock may
evaluate repeatedly or out of order).

### `RunResult` — structured assertions

- `.stdout()`, `.stderr()`, `.success()`, `.assert_success()`
- `.events() -> &[AgentEvent]` — parses the `--json-events` NDJSON into the real
  `codel00p_protocol::AgentEvent` type.
- Matchers: `.assert_tool_called(name)` (request + completion),
  `.assert_tool_requested/completed(name)`, `.assert_turn_completed()`,
  `.assert_event(predicate)`, `.has_event(predicate)`,
  `.context_manifest()` / `.assert_context_manifest()`.

DB and workspace assertions are done with the real types directly in the test
(see `tests/pilot.rs`): persisted memory candidates are read through
`codel00p_memory::StorageBackedMemoryStore`, and workspace/git side effects are
checked against `runner.workspace_path()`.

## Binary resolution

`assert_cmd::cargo_bin("codel00p")` does **not** work from this crate: the
`CARGO_BIN_EXE_codel00p` env var is only injected for the crate that declares the
binary target (`codel00p-cli`), and a plain dependency on that binary-only crate
is *ignored* by Cargo ("missing a lib target") so it does not force a build.

The harness therefore resolves the binary from the workspace target directory
relative to `CARGO_MANIFEST_DIR` (`<workspace>/target/<profile>/codel00p`,
honoring `CARGO_TARGET_DIR`), and **builds it on demand once** via `cargo build
-p codel00p-cli --bin codel00p` if it is missing. CI builds it explicitly first,
so there the build is a fast no-op. See `codel00p_e2e::codel00p_binary`. A
`tests/smoke.rs` test guards this resolution and runs first.

## Honest limitations / notes

- **Memory IS wired** in the `agent run` CLI path
  (`turn_memory_extractor` + `memory_candidate_sink` in
  `codel00p-cli/src/agent/turn.rs`), so a `remember <kind>[tags]: content`
  directive in the final assistant text becomes a persisted memory **candidate**
  (status `Candidate`). The pilot asserts this through the real memory store.
- The `git_commit` tool only commits **tracked** changes and refuses to run with
  untracked files present. The pilot stages the new file with a `run_command`
  `git add` step before committing — a realistic flow.
- This harness is intentionally offline-only. To exercise a live provider, invoke
  the binary directly with a real `--provider`/key (see `examples/repl.rs`).

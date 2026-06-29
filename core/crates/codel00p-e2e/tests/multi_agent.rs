//! Multi-agent personas end-to-end tests (initiative #13, phase 4).
//!
//! Proves the full multi-agent + memory-switch journey through the **real**
//! `codel00p` binary + a scripted mock provider, fully hermetic (no network,
//! isolated `CODEL00P_HOME` + workspace per scenario).
//!
//! An *agent* is a directory `<base>/agents/<name>/` used as its own
//! `CODEL00P_HOME`. The active agent (sticky `<base>/active_agent` pointer set by
//! `agent use`, or a one-shot `--agent` flag) determines which agent's
//! `memory.sqlite`, sessions, and `persona.md` a run uses. The default/base agent
//! is the bare base home.
//!
//! # Why these runs bypass the harness's `--memory-db` injection
//!
//! [`CodelRunner::run`] always appends `--memory-db <base>/memory.sqlite`. That
//! explicit flag wins over home resolution in the CLI
//! (`resolve_cli_config`: `overrides.memory_db.unwrap_or_else(...)`), so *every*
//! agent would write to the BASE db and the per-agent isolation this suite must
//! prove would silently collapse. The multi-agent switch is structural: it works
//! by pointing `CODEL00P_HOME` at the agent home and letting the memory/session
//! store resolve `<home>/memory.sqlite` — exactly what a live switch does and
//! what the CLI integration tests in `codel00p-cli/tests/agent_cli/memory_switch.rs`
//! exercise (they too pass NO `--memory-db`).
//!
//! So agent runs here go through [`CodelRunner::run_plain`] (which sets the
//! isolated `CODEL00P_HOME` + provider key but injects *no* global flags), with
//! the provider/model/base-url/json-events/permission flags assembled by
//! [`agent_run`] — and crucially **no** `--memory-db`. Management commands
//! (`agent create|use|list`) also use `run_plain` since they reject run-only
//! flags. The org/project flags are still passed so the memory storage scope
//! (`org-1`/`project-1`) stays stable across runs and matches [`candidates_in`].
//!
//! # No `#[ignore]`s here
//!
//! Every scenario uses the explicit `remember:` directive (wired in the CLI's
//! turn memory extractor), so the suite covers everything without relying on the
//! post-session recommender (which is NOT wired in the CLI path — see
//! `memory_loop.rs`).

use std::path::{Path, PathBuf};

use codel00p_e2e::{CodelRunner, MockProvider, RunResult};
use codel00p_memory::{MemoryListFilter, MemoryRepository, StorageBackedMemoryStore};
use codel00p_protocol::{MemoryStatus, ProjectRef};
use codel00p_storage::{SqliteStorage, StorageScope};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The base home's `memory.sqlite` is at `home/memory.sqlite`; a named agent's
/// lives at `home/agents/<name>/memory.sqlite`.
fn agent_db(runner: &CodelRunner, name: &str) -> PathBuf {
    runner
        .home_path()
        .join("agents")
        .join(name)
        .join("memory.sqlite")
}

/// Opens the memory store at `db_path` and returns `(id, content)` for every
/// candidate-status entry in the `org-1`/`project-1` scope. Mirrors the
/// `list_candidates` helper in `memory_loop.rs`, parameterized by an explicit db
/// path so it can target any agent's (or the base's) store.
///
/// A missing db is treated as "no candidates" — switching to a brand-new agent
/// that has never run leaves no `memory.sqlite` on disk, which must read as empty
/// rather than panic.
fn candidates_in(db_path: &Path) -> Vec<(String, String)> {
    if !db_path.exists() {
        return Vec::new();
    }
    let storage = SqliteStorage::open(db_path).expect("open memory.sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let project = ProjectRef::new("project-1", "codel00p");
    let filter = MemoryListFilter::new(project).with_status(MemoryStatus::Candidate);
    store
        .list(filter)
        .expect("list memory candidates")
        .into_iter()
        .map(|rec| {
            (
                rec.entry().id().to_string(),
                rec.entry().content().to_string(),
            )
        })
        .collect()
}

/// Whether any candidate at `db_path` contains `needle`.
fn db_contains(db_path: &Path, needle: &str) -> bool {
    candidates_in(db_path)
        .iter()
        .any(|(_, content)| content.contains(needle))
}

/// Drive a real `agent run` through `run_plain`, with provider wiring assembled
/// manually and **no** `--memory-db` override, so memory/sessions resolve under
/// whichever agent home is active (sticky pointer or `--agent`). See the module
/// docs for why the standard `CodelRunner::run` is unusable here.
///
/// `agent_flag` (when `Some`) is passed as the leading global `--agent <name>`
/// flag (one-shot selection, no sticky pointer). Global flags are position-tolerant
/// (see `agent_flag_after_subcommand_selects_home` for the trailing case), but this
/// helper leads with them for clarity.
fn agent_run(
    runner: &CodelRunner,
    provider: &MockProvider,
    agent_flag: Option<&str>,
    prompt: &str,
) -> RunResult {
    let base_url = provider.base_url();
    let workspace = runner.workspace_path().to_str().expect("utf8 workspace");
    let mut args: Vec<&str> = vec![
        "--organization-id",
        "org-1",
        "--project-id",
        "project-1",
        "--project-name",
        "codel00p",
    ];
    if let Some(name) = agent_flag {
        args.push("--agent");
        args.push(name);
    }
    args.extend_from_slice(&[
        "agent",
        "run",
        prompt,
        "--workspace",
        workspace,
        "--provider",
        "custom",
        "--model",
        "test-model",
        "--base-url",
        &base_url,
        "--json-events",
        "--permission-mode",
        "allow",
    ]);
    runner.run_plain(&args)
}

/// Extract the system-prompt text (`messages[0].content`) from a captured model
/// request body. The persona + self block are rendered into the first system
/// message, mirroring the `extract_system_content` helper in `tests/context.rs`.
fn system_content(request_body: &str) -> String {
    let body: serde_json::Value =
        serde_json::from_str(request_body).expect("model request must be valid JSON");
    body["messages"][0]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// Scenario 1 — Lifecycle: create, list, on-disk layout
// ---------------------------------------------------------------------------

/// `agent create coder` + `agent create reviewer` make both agents listable
/// alongside the implicit `default`, and each agent home is seeded with
/// `agent.toml`, `config.toml`, and `persona.md`.
#[test]
fn lifecycle_create_list_and_agent_dirs() {
    let runner = CodelRunner::new();

    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();
    runner
        .run_plain(&["agent", "create", "reviewer"])
        .assert_success();

    let list = runner.run_plain(&["agent", "list"]);
    list.assert_success();
    let out = list.stdout();
    assert!(
        out.contains("default"),
        "agent list must show the implicit default, got:\n{out}"
    );
    assert!(
        out.contains("coder"),
        "agent list must show `coder`, got:\n{out}"
    );
    assert!(
        out.contains("reviewer"),
        "agent list must show `reviewer`, got:\n{out}"
    );

    for name in ["coder", "reviewer"] {
        let home = runner.home_path().join("agents").join(name);
        for file in ["agent.toml", "config.toml", "persona.md"] {
            assert!(
                home.join(file).is_file(),
                "agent `{name}` home should contain {file} at {}",
                home.join(file).display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Scenario 2 — Memory isolation + persistence across switches (THE core proof)
//
// Each agent's `remember:` directive must land ONLY in that agent's
// `memory.sqlite`; switching agents must not leak, and an agent's memory must
// survive a round-trip switch. The base/default agent sees neither.
// ---------------------------------------------------------------------------

#[test]
fn memory_isolates_and_persists_across_switches() {
    let runner = CodelRunner::new();

    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();
    runner
        .run_plain(&["agent", "create", "reviewer"])
        .assert_success();

    let coder_db = agent_db(&runner, "coder");
    let reviewer_db = agent_db(&runner, "reviewer");
    let base_db = runner.memory_db();

    // --- coder remembers its convention ---
    runner
        .run_plain(&["agent", "use", "coder"])
        .assert_success();
    let coder_provider = MockProvider::start()
        .assistant_text("Done.\nremember convention[coder]: coder uses tabs for indentation.");
    agent_run(
        &runner,
        &coder_provider,
        None,
        "Record the coder indentation convention.",
    )
    .assert_success();

    assert!(
        db_contains(&coder_db, "coder uses tabs"),
        "coder's remember: directive must land in coder's db ({}), got: {:?}",
        coder_db.display(),
        candidates_in(&coder_db)
    );

    // --- switch to reviewer: it must NOT see coder's memory ---
    runner
        .run_plain(&["agent", "use", "reviewer"])
        .assert_success();
    assert!(
        !db_contains(&reviewer_db, "coder uses tabs"),
        "reviewer must NOT see coder's memory (isolation), got: {:?}",
        candidates_in(&reviewer_db)
    );

    let reviewer_provider = MockProvider::start().assistant_text(
        "Done.\nremember convention[reviewer]: reviewer uses spaces for indentation.",
    );
    agent_run(
        &runner,
        &reviewer_provider,
        None,
        "Record the reviewer indentation convention.",
    )
    .assert_success();

    // reviewer's db has ONLY reviewer's fact.
    assert!(
        db_contains(&reviewer_db, "reviewer uses spaces"),
        "reviewer's remember: directive must land in reviewer's db, got: {:?}",
        candidates_in(&reviewer_db)
    );
    assert!(
        !db_contains(&reviewer_db, "coder uses tabs"),
        "reviewer's db must contain ONLY reviewer's fact, got: {:?}",
        candidates_in(&reviewer_db)
    );

    // --- switch back to coder: its memory PERSISTS and reviewer's is absent ---
    runner
        .run_plain(&["agent", "use", "coder"])
        .assert_success();
    assert!(
        db_contains(&coder_db, "coder uses tabs"),
        "coder's memory must persist across the switch, got: {:?}",
        candidates_in(&coder_db)
    );
    assert!(
        !db_contains(&coder_db, "reviewer uses spaces"),
        "coder's db must NOT contain reviewer's fact, got: {:?}",
        candidates_in(&coder_db)
    );

    // --- the base/default agent's db has neither fact ---
    assert!(
        !db_contains(&base_db, "coder uses tabs") && !db_contains(&base_db, "reviewer uses spaces"),
        "the default/base agent must see neither agent's memory, got: {:?}",
        candidates_in(&base_db)
    );

    // The two per-agent dbs are physically distinct files under their homes.
    assert!(
        coder_db.is_file(),
        "coder db should exist at {}",
        coder_db.display()
    );
    assert!(
        reviewer_db.is_file(),
        "reviewer db should exist at {}",
        reviewer_db.display()
    );
}

// ---------------------------------------------------------------------------
// Scenario 3 — Per-agent sessions
//
// A session created during a run under `coder` is listed under `coder` but NOT
// under `reviewer` (separate per-agent session stores resolved under each home).
// ---------------------------------------------------------------------------

#[test]
fn sessions_isolate_per_agent() {
    let runner = CodelRunner::new();

    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();
    runner
        .run_plain(&["agent", "create", "reviewer"])
        .assert_success();

    // Run a turn under coder so a session is persisted in coder's home. `agent
    // run` records under the `org-1`/`project-1` scope (the flags `agent_run`
    // passes), so `session list` must carry the same scope flags to find it —
    // `session list` is scope-sensitive and the bare default scope is empty.
    runner
        .run_plain(&["agent", "use", "coder"])
        .assert_success();
    let provider = MockProvider::start().assistant_text("Coder session established.");
    agent_run(&runner, &provider, None, "Start the coder session.").assert_success();

    // coder's session store now lists at least one session.
    let coder_sessions = session_list(&runner);
    coder_sessions.assert_success();
    let coder_listing = coder_sessions.stdout().to_string();
    let coder_ids = session_ids(&coder_listing);
    assert!(
        !coder_ids.is_empty(),
        "coder should have at least one session listed, got:\n{coder_listing}"
    );

    // The coder session db exists under coder's home (sessions share memory.sqlite).
    let coder_db = agent_db(&runner, "coder");
    assert!(
        coder_db.is_file(),
        "coder's session/memory store should exist at {}",
        coder_db.display()
    );

    // --- switch to reviewer: coder's session(s) must NOT appear ---
    runner
        .run_plain(&["agent", "use", "reviewer"])
        .assert_success();
    let reviewer_sessions = session_list(&runner);
    reviewer_sessions.assert_success();
    let reviewer_listing = reviewer_sessions.stdout();

    // reviewer has its own (empty) store — none of coder's session ids leak.
    for id in &coder_ids {
        assert!(
            !reviewer_listing.contains(id),
            "reviewer must not list coder's session `{id}` (isolation), got:\n{reviewer_listing}"
        );
    }

    // The reviewer store is physically distinct (or absent: a fresh agent with no
    // run yet leaves no db), proving the stores do not share state.
    let reviewer_db = agent_db(&runner, "reviewer");
    assert_ne!(
        coder_db, reviewer_db,
        "coder and reviewer must resolve to different session stores"
    );
}

/// `session list` carrying the same `org-1`/`project-1` scope that `agent_run`
/// records under (the listing is scope-sensitive; the bare default scope is
/// empty). Not a run subcommand, so it goes through `run_plain`.
fn session_list(runner: &CodelRunner) -> RunResult {
    runner.run_plain(&[
        "--organization-id",
        "org-1",
        "--project-id",
        "project-1",
        "--project-name",
        "codel00p",
        "session",
        "list",
    ])
}

/// The session ids in a `session list` listing — the first whitespace-delimited
/// token of each line beginning with `session-`.
fn session_ids(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|token| token.starts_with("session-"))
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// Scenario 4 — Persona per agent
//
// Each agent's `persona.md` is injected into the model request, and the self
// block names the active agent. The default agent injects no persona.
// ---------------------------------------------------------------------------

#[test]
fn persona_per_agent_reaches_model_request() {
    let runner = CodelRunner::new();

    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();
    runner
        .run_plain(&["agent", "create", "reviewer"])
        .assert_success();

    // Distinct, unmistakable personas per agent.
    let coder_persona = "# Persona: coder\nI am CODER_PERSONA_MARKER, a meticulous implementer.\n";
    let reviewer_persona = "# Persona: reviewer\nI am REVIEWER_PERSONA_MARKER, a careful critic.\n";
    std::fs::write(
        runner
            .home_path()
            .join("agents")
            .join("coder")
            .join("persona.md"),
        coder_persona,
    )
    .expect("write coder persona");
    std::fs::write(
        runner
            .home_path()
            .join("agents")
            .join("reviewer")
            .join("persona.md"),
        reviewer_persona,
    )
    .expect("write reviewer persona");

    // --- coder run: coder's persona + identity reach the request ---
    runner
        .run_plain(&["agent", "use", "coder"])
        .assert_success();
    let coder_provider = MockProvider::start().assistant_text("Implemented.");
    agent_run(&runner, &coder_provider, None, "Do the coder task.").assert_success();
    let coder_requests = coder_provider.received_requests();
    assert!(
        !coder_requests.is_empty(),
        "coder run must have hit the mock provider"
    );
    let coder_system = system_content(&coder_requests[0]);
    assert!(
        coder_system.contains("CODER_PERSONA_MARKER"),
        "coder's persona text must reach the model request, got:\n{coder_system}"
    );
    assert!(
        !coder_system.contains("REVIEWER_PERSONA_MARKER"),
        "coder's request must NOT contain reviewer's persona, got:\n{coder_system}"
    );
    assert!(
        coder_system.contains("coder"),
        "the self block must name the active agent `coder`, got:\n{coder_system}"
    );

    // --- reviewer run: reviewer's persona + identity reach the request ---
    runner
        .run_plain(&["agent", "use", "reviewer"])
        .assert_success();
    let reviewer_provider = MockProvider::start().assistant_text("Reviewed.");
    agent_run(&runner, &reviewer_provider, None, "Do the reviewer task.").assert_success();
    let reviewer_requests = reviewer_provider.received_requests();
    assert!(
        !reviewer_requests.is_empty(),
        "reviewer run must have hit the mock provider"
    );
    let reviewer_system = system_content(&reviewer_requests[0]);
    assert!(
        reviewer_system.contains("REVIEWER_PERSONA_MARKER"),
        "reviewer's persona text must reach the model request, got:\n{reviewer_system}"
    );
    assert!(
        !reviewer_system.contains("CODER_PERSONA_MARKER"),
        "reviewer's request must NOT contain coder's persona, got:\n{reviewer_system}"
    );
    assert!(
        reviewer_system.contains("reviewer"),
        "the self block must name the active agent `reviewer`, got:\n{reviewer_system}"
    );

    // --- default agent: no persona injected ---
    runner
        .run_plain(&["agent", "use", "--default"])
        .assert_success();
    let default_provider = MockProvider::start().assistant_text("Default done.");
    agent_run(&runner, &default_provider, None, "Do a default task.").assert_success();
    let default_requests = default_provider.received_requests();
    assert!(
        !default_requests.is_empty(),
        "default run must have hit the mock provider"
    );
    let default_system = system_content(&default_requests[0]);
    assert!(
        !default_system.contains("CODER_PERSONA_MARKER")
            && !default_system.contains("REVIEWER_PERSONA_MARKER"),
        "the default agent must inject no persona, got:\n{default_system}"
    );
    // The default agent renders the product identity, not an agent persona block.
    assert!(
        default_system.contains("You are codel00p v"),
        "the default agent should render the product self identity, got:\n{default_system}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5 — `--agent` flag path (one-shot, no sticky pointer)
//
// `--agent coder` selects coder's home for a single run WITHOUT writing the
// sticky pointer. The remembered fact lands in coder's db, and the active
// pointer is never set (the default stays default).
// ---------------------------------------------------------------------------

#[test]
fn agent_flag_selects_home_without_sticky_pointer() {
    let runner = CodelRunner::new();

    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();

    // No `agent use`: the sticky pointer is unset; we drive a one-shot via --agent.
    let provider = MockProvider::start()
        .assistant_text("Done.\nremember convention[flag]: FLAG_PATH_FACT via the --agent flag.");
    agent_run(
        &runner,
        &provider,
        Some("coder"),
        "Record a fact via the --agent flag.",
    )
    .assert_success();

    // The fact landed in coder's db (the flag repointed the home).
    let coder_db = agent_db(&runner, "coder");
    assert!(
        db_contains(&coder_db, "FLAG_PATH_FACT"),
        "--agent coder must write to coder's db, got: {:?}",
        candidates_in(&coder_db)
    );

    // The base/default db did NOT receive it (the flag did not pollute the base).
    assert!(
        !db_contains(&runner.memory_db(), "FLAG_PATH_FACT"),
        "--agent run must not write to the base db, got: {:?}",
        candidates_in(&runner.memory_db())
    );

    // The sticky pointer was never written by a `--agent` flag run.
    assert!(
        !runner.home_path().join("active_agent").exists(),
        "a one-shot --agent run must NOT set the sticky active_agent pointer"
    );

    // The self block named coder even though no pointer was set.
    let requests = provider.received_requests();
    assert!(
        !requests.is_empty(),
        "the --agent run must have hit the mock"
    );
    let system = system_content(&requests[0]);
    assert!(
        system.contains("coder"),
        "the --agent flag run should name `coder` in the self block, got:\n{system}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 7 — Position-tolerant global `--agent` flag
//
// The global `--agent <name>` flag may appear AFTER the subcommand, not just
// leading. This is the exact dogfooding papercut (`agent run … --agent coder`
// used to error "unknown agent run option"). The run must select coder's home
// and land the remembered fact in coder's db.
// ---------------------------------------------------------------------------

#[test]
fn agent_flag_after_subcommand_selects_home() {
    let runner = CodelRunner::new();
    runner
        .run_plain(&["agent", "create", "coder"])
        .assert_success();

    let provider = MockProvider::start().assistant_text(
        "Done.\nremember convention[trailing]: TRAILING_FLAG_FACT after the subcommand.",
    );
    let base_url = provider.base_url();
    let workspace = runner.workspace_path().to_str().expect("utf8 workspace");

    // `--agent coder` is placed at the VERY END, after every run flag.
    let result = runner.run_plain(&[
        "--organization-id",
        "org-1",
        "--project-id",
        "project-1",
        "--project-name",
        "codel00p",
        "agent",
        "run",
        "Record a fact with a trailing --agent flag.",
        "--workspace",
        workspace,
        "--provider",
        "custom",
        "--model",
        "test-model",
        "--base-url",
        &base_url,
        "--json-events",
        "--permission-mode",
        "allow",
        "--agent",
        "coder",
    ]);
    result.assert_success();

    // The fact landed in coder's db — the trailing flag repointed the home.
    let coder_db = agent_db(&runner, "coder");
    assert!(
        db_contains(&coder_db, "TRAILING_FLAG_FACT"),
        "trailing --agent coder must write to coder's db, got: {:?}",
        candidates_in(&coder_db)
    );
    // The base db did not receive it.
    assert!(
        !db_contains(&runner.memory_db(), "TRAILING_FLAG_FACT"),
        "trailing --agent run must not write to the base db, got: {:?}",
        candidates_in(&runner.memory_db())
    );
}

// ---------------------------------------------------------------------------
// Scenario 6 — Default / back-compat
//
// With no agent created or selected, a run uses the base home `memory.sqlite`
// (today's behavior). The agents dir is never created.
// ---------------------------------------------------------------------------

#[test]
fn default_run_uses_base_home_memory() {
    let runner = CodelRunner::new();

    let provider = MockProvider::start().assistant_text(
        "Done.\nremember convention[default]: DEFAULT_BASE_FACT for the base home.",
    );
    agent_run(&runner, &provider, None, "Record a base-home fact.").assert_success();

    // The fact landed in the base home's db.
    assert!(
        db_contains(&runner.memory_db(), "DEFAULT_BASE_FACT"),
        "a default run must write to the base home db, got: {:?}",
        candidates_in(&runner.memory_db())
    );

    // No agents were created, so no agents dir exists.
    assert!(
        !runner.home_path().join("agents").exists(),
        "a default run must not create an agents/ directory"
    );

    // The default run renders the product identity, no persona block.
    let requests = provider.received_requests();
    assert!(
        !requests.is_empty(),
        "the default run must have hit the mock"
    );
    let system = system_content(&requests[0]);
    assert!(
        system.contains("You are codel00p v"),
        "the default run should render the product self identity, got:\n{system}"
    );
}

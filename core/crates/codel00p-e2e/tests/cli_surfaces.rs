//! End-to-end black-box smoke/behavior tests for the non-agent CLI surfaces.
//!
//! These drive the **real** `codel00p` binary as a subprocess (via the shared
//! [`CodelRunner`] harness) and assert the actually-observed output and exit
//! status of the scriptable, non-agent subcommands:
//!
//! - `version` / `--version` — prints the build version.
//! - `--help` / `<command> --help` — lists the command tree.
//! - `config` — `init` / `set` / `get` / `show` round-trip under the isolated
//!   `CODEL00P_HOME`, plus the unknown-key error path.
//! - `config providers` — lists known providers and marks the default.
//! - `skills` — `list` / `create` / `show` round-trip, plus unknown-skill error.
//! - `cron` — `add` → `list` → `show` → `run` (a real agent turn against a mock
//!   provider) → `remove`.
//! - `cloud` — offline `status` error path; `push`/`pull` against a mock cloud.
//! - `update` — hermetic "up to date" report via an explicit older `--version`
//!   (no network, no self-update).
//!
//! # Invocation discipline (critical)
//!
//! The harness only auto-injects provider flags for `agent` subcommands, so
//! these non-agent commands use plain [`CodelRunner::run`] (no
//! `--provider/--model/--base-url/--json-events`). However, [`CodelRunner::run`]
//! still prepends the four global flags (`--memory-db`, `--organization-id`,
//! `--project-id`, `--project-name`). That is fine for every command that flows
//! through the global-flag parser (config, providers, skills, cron, cloud,
//! update). It is NOT fine for `version`/`--help`, which the binary detects by
//! inspecting the *raw* leading argv before the parser runs — a prepended
//! `--memory-db` would shift those positions and make them fall through to
//! "unknown command". Those two surfaces therefore use [`CodelRunner::run_plain`]
//! (no injected flags at all).
//!
//! Fully hermetic: no network access and no real credentials. Surfaces that
//! genuinely require the network assert their offline/error path instead.

use codel00p_e2e::CodelRunner;
use codel00p_memory::{MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};
use codel00p_storage::{SqliteStorage, StorageScope};
use httpmock::prelude::*;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// version / --version
// ---------------------------------------------------------------------------

#[test]
fn version_subcommand_prints_a_version() {
    let runner = CodelRunner::new();
    // `version` is matched from the raw leading argv, so it must be invoked
    // without the runner's injected global flags.
    let result = runner.run_plain(&["version"]);
    result.assert_success();
    let out = result.stdout();
    assert!(
        out.starts_with("codel00p "),
        "version should print `codel00p <semver>`; got: {out:?}"
    );
    // The version token after the name parses as `major.minor.patch`.
    let token = out
        .trim()
        .strip_prefix("codel00p ")
        .expect("version line has a token after the name");
    let mut parts = token.split('.');
    for label in ["major", "minor", "patch"] {
        let part = parts
            .next()
            .unwrap_or_else(|| panic!("version token {token:?} missing {label} component"));
        part.parse::<u64>()
            .unwrap_or_else(|_| panic!("version {label} component {part:?} is not numeric"));
    }
}

#[test]
fn version_flag_matches_subcommand() {
    let runner = CodelRunner::new();
    let flag = runner.run_plain(&["--version"]);
    flag.assert_success();
    let sub = runner.run_plain(&["version"]);
    sub.assert_success();
    assert_eq!(
        flag.stdout(),
        sub.stdout(),
        "`--version` and `version` should print identically"
    );
}

// ---------------------------------------------------------------------------
// --help / <command> --help
// ---------------------------------------------------------------------------

#[test]
fn top_level_help_lists_the_command_tree() {
    let runner = CodelRunner::new();
    let result = runner.run_plain(&["--help"]);
    result.assert_success();
    let help = result.stdout();
    assert!(help.contains("Usage"), "help should have a Usage section");
    assert!(
        help.contains("codel00p [options] [command]"),
        "help should show the top-level usage line; got:\n{help}"
    );
    // Each documented non-agent subcommand appears in the listing.
    for command in [
        "agent", "config", "auth", "cloud", "session", "memory", "skills", "cron", "gateway",
        "mcp", "update", "version",
    ] {
        assert!(
            help.contains(command),
            "top-level help should list `{command}`; got:\n{help}"
        );
    }
}

#[test]
fn command_help_prints_usage_for_each_surface() {
    let runner = CodelRunner::new();
    // `<command> --help` is matched on the raw argv slice, so no global flags.
    for (args, needle) in [
        (
            &["config", "--help"][..],
            "Usage: codel00p config <command>",
        ),
        (
            &["config", "providers", "--help"][..],
            "Usage: codel00p config providers <command>",
        ),
        (&["cron", "--help"][..], "codel00p"),
        (&["cloud", "--help"][..], "codel00p"),
        (&["update", "--help"][..], "Usage: codel00p update"),
        (&["skills", "--help"][..], "codel00p"),
    ] {
        let result = runner.run_plain(args);
        result.assert_success();
        assert!(
            result.stdout().contains(needle),
            "`{args:?}` help should contain {needle:?}; got:\n{}",
            result.stdout()
        );
    }
}

// ---------------------------------------------------------------------------
// config: init / set / get / show round-trip
// ---------------------------------------------------------------------------

#[test]
fn config_init_set_get_show_round_trip() {
    let runner = CodelRunner::new();

    let init = runner.run(&["config", "init"]);
    init.assert_success();
    assert!(
        runner.home_path().join("config.toml").exists(),
        "config init should create config.toml under CODEL00P_HOME"
    );

    runner
        .run(&["config", "set", "agent.provider", "openrouter"])
        .assert_success();

    let get = runner.run(&["config", "get", "agent.provider"]);
    get.assert_success();
    assert_eq!(
        get.stdout().trim(),
        "openrouter",
        "config get should reflect the value just set"
    );

    let show = runner.run(&["config", "show"]);
    show.assert_success();
    assert!(
        show.stdout().contains("codel00p configuration"),
        "config show should print the configuration banner; got:\n{}",
        show.stdout()
    );
    assert!(
        show.stdout().contains("openrouter"),
        "config show should reflect the configured provider; got:\n{}",
        show.stdout()
    );
}

#[test]
fn memory_note_add_and_show_round_trip() {
    let runner = CodelRunner::new();

    // Append an agent note and a user note.
    runner
        .run(&["memory", "note", "the project uses cargo"])
        .assert_success();
    runner
        .run(&["memory", "note", "--user", "prefers terse answers"])
        .assert_success();

    // The files land in the (default agent) base home.
    assert!(
        runner.home_path().join("NOTES.md").exists(),
        "memory note should create NOTES.md under CODEL00P_HOME"
    );
    assert!(
        runner.home_path().join("USER.md").exists(),
        "memory note --user should create USER.md under CODEL00P_HOME"
    );

    // `--show` prints both files' current contents.
    let show = runner.run(&["memory", "note", "--show"]);
    show.assert_success();
    assert!(
        show.stdout().contains("the project uses cargo"),
        "show should print the agent note; got:\n{}",
        show.stdout()
    );
    assert!(
        show.stdout().contains("prefers terse answers"),
        "show should print the user note; got:\n{}",
        show.stdout()
    );
}

#[test]
fn memory_note_is_per_agent() {
    let runner = CodelRunner::new();

    // Notes written under agent `alpha`...
    runner
        .run(&["--agent", "alpha", "memory", "note", "alpha-only fact"])
        .assert_success();
    // ...and a different note under agent `beta`.
    runner
        .run(&["--agent", "beta", "memory", "note", "beta-only fact"])
        .assert_success();

    // Each agent only sees its own note (homes isolate via CODEL00P_HOME).
    let alpha = runner.run(&["--agent", "alpha", "memory", "note", "--show"]);
    alpha.assert_success();
    assert!(
        alpha.stdout().contains("alpha-only fact"),
        "alpha should see its own note; got:\n{}",
        alpha.stdout()
    );
    assert!(
        !alpha.stdout().contains("beta-only fact"),
        "alpha must not see beta's note; got:\n{}",
        alpha.stdout()
    );

    let beta = runner.run(&["--agent", "beta", "memory", "note", "--show"]);
    beta.assert_success();
    assert!(
        beta.stdout().contains("beta-only fact"),
        "beta should see its own note; got:\n{}",
        beta.stdout()
    );
    assert!(
        !beta.stdout().contains("alpha-only fact"),
        "beta must not see alpha's note; got:\n{}",
        beta.stdout()
    );

    // On disk, each agent home has its own NOTES.md.
    assert!(runner.home_path().join("agents/alpha/NOTES.md").exists());
    assert!(runner.home_path().join("agents/beta/NOTES.md").exists());
}

#[test]
fn config_set_rejects_unknown_key() {
    let runner = CodelRunner::new();
    let result = runner.run(&["config", "set", "agent.bogus", "x"]);
    assert!(
        !result.success(),
        "setting an unknown config key should fail"
    );
    assert!(
        result.stderr().contains("unknown config key"),
        "stderr should explain the unknown key; got:\n{}",
        result.stderr()
    );
}

// ---------------------------------------------------------------------------
// config providers: list
// ---------------------------------------------------------------------------

#[test]
fn providers_list_shows_known_providers() {
    let runner = CodelRunner::new();
    let result = runner.run(&["config", "providers", "list"]);
    result.assert_success();
    let listing = result.stdout();
    assert!(
        listing.contains("Providers"),
        "providers list should have a `Providers` header; got:\n{listing}"
    );
    // A few well-known providers are always present in the registry.
    for provider in ["anthropic", "custom", "gemini"] {
        assert!(
            listing.contains(provider),
            "providers list should include `{provider}`; got:\n{listing}"
        );
    }
}

#[test]
fn providers_use_marks_the_default() {
    let runner = CodelRunner::new();
    runner
        .run(&[
            "config",
            "providers",
            "use",
            "custom",
            "--model",
            "test-model",
        ])
        .assert_success();

    let model = runner.run(&["config", "get", "agent.model"]);
    model.assert_success();
    assert_eq!(model.stdout().trim(), "test-model");

    let list = runner.run(&["config", "providers", "list"]);
    list.assert_success();
    assert!(
        list.stdout().contains("(default)"),
        "after `providers use`, the list should mark a default; got:\n{}",
        list.stdout()
    );
}

// ---------------------------------------------------------------------------
// skills: list / create / show round-trip
// ---------------------------------------------------------------------------

#[test]
fn skills_list_is_empty_then_create_show_round_trip() {
    let runner = CodelRunner::new();

    let empty = runner.run(&["skills", "list"]);
    empty.assert_success();
    assert!(
        empty.stdout().contains("No skills found"),
        "a fresh home has no skills; got:\n{}",
        empty.stdout()
    );

    let create = runner.run(&["skills", "create", "deploy"]);
    create.assert_success();
    assert!(
        create.stdout().contains("Created skill deploy"),
        "create should confirm; got:\n{}",
        create.stdout()
    );
    assert!(
        runner.home_path().join("skills/deploy/SKILL.md").exists(),
        "create should scaffold a SKILL.md"
    );

    let list = runner.run(&["skills", "list"]);
    list.assert_success();
    assert!(
        list.stdout().contains("deploy"),
        "list should now include the new skill; got:\n{}",
        list.stdout()
    );

    let show = runner.run(&["skills", "show", "deploy"]);
    show.assert_success();
    assert!(
        show.stdout().contains("deploy (user)"),
        "show should label the user skill; got:\n{}",
        show.stdout()
    );
}

#[test]
fn skills_show_unknown_errors() {
    let runner = CodelRunner::new();
    let result = runner.run(&["skills", "show", "nope"]);
    assert!(!result.success(), "showing an unknown skill should fail");
    assert!(
        result.stderr().contains("unknown skill: nope"),
        "stderr should name the unknown skill; got:\n{}",
        result.stderr()
    );
}

// ---------------------------------------------------------------------------
// cron: add → list → show → run → remove
// ---------------------------------------------------------------------------

#[test]
fn cron_add_list_show_remove_round_trip() {
    let runner = CodelRunner::new();

    let empty = runner.run(&["cron", "list"]);
    empty.assert_success();
    assert!(
        empty.stdout().contains("No scheduled jobs"),
        "a fresh home has no cron jobs; got:\n{}",
        empty.stdout()
    );

    let add = runner.run(&["cron", "add", "30m", "Run", "the", "checks"]);
    add.assert_success();
    assert!(
        add.stdout().contains("Added cron-1 (every 30m)"),
        "add should confirm with the assigned id and schedule; got:\n{}",
        add.stdout()
    );

    let list = runner.run(&["cron", "list"]);
    list.assert_success();
    assert!(
        list.stdout().contains("cron-1") && list.stdout().contains("every 30m"),
        "list should show the added job; got:\n{}",
        list.stdout()
    );

    let show = runner.run(&["cron", "show", "cron-1"]);
    show.assert_success();
    assert!(
        show.stdout().contains("Run the checks"),
        "show should print the job prompt; got:\n{}",
        show.stdout()
    );

    let remove = runner.run(&["cron", "remove", "cron-1"]);
    remove.assert_success();
    assert!(
        remove.stdout().contains("Removed cron-1"),
        "remove should confirm; got:\n{}",
        remove.stdout()
    );
    let after = runner.run(&["cron", "list"]);
    after.assert_success();
    assert!(
        after.stdout().contains("No scheduled jobs"),
        "list should be empty after removal; got:\n{}",
        after.stdout()
    );
}

#[test]
fn cron_add_rejects_a_bad_schedule() {
    let runner = CodelRunner::new();
    let result = runner.run(&["cron", "add", "soon", "do", "it"]);
    assert!(!result.success(), "an invalid schedule should be rejected");
    assert!(
        result.stderr().contains("invalid schedule"),
        "stderr should explain the bad schedule; got:\n{}",
        result.stderr()
    );
}

#[test]
fn cron_run_executes_a_job_as_an_agent_turn() {
    // `cron run` resolves the configured provider and runs a real agent turn, so
    // it needs a model endpoint. We point the agent at a mock chat-completions
    // server via `config set` (no network), then run the job and assert the
    // model's reply surfaces. This is the documented hermetic path for `run`.
    let runner = CodelRunner::new();

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": { "role": "assistant", "content": "ran the nightly job" },
                "finish_reason": "stop"
            }]
        }));
    });

    runner
        .run(&["config", "set", "agent.provider", "custom"])
        .assert_success();
    runner
        .run(&["config", "set", "agent.model", "test-model"])
        .assert_success();
    runner
        .run(&["config", "set", "agent.base_url", &server.base_url()])
        .assert_success();

    runner
        .run(&["cron", "add", "1h", "summarize", "the", "day"])
        .assert_success();

    let run = runner.run(&["cron", "run", "cron-1"]);
    run.assert_success();
    assert!(
        run.stdout().contains("ran the nightly job"),
        "cron run should surface the agent reply; got:\n{}",
        run.stdout()
    );
}

// ---------------------------------------------------------------------------
// cloud: offline status (error path) + push/pull against a mock cloud
// ---------------------------------------------------------------------------

#[test]
fn cloud_status_offline_reports_missing_connection() {
    // With no `--api-url`/`--token` and no stored credentials, `cloud status`
    // cannot reach a cloud and must fail with a friendly message. We assert the
    // real offline error path rather than faking a connection.
    let runner = CodelRunner::new();
    let result = runner.run(&["cloud", "status"]);
    assert!(
        !result.success(),
        "offline `cloud status` should fail without connection details"
    );
    assert!(
        result.stderr().contains("--api-url"),
        "stderr should point at --api-url; got:\n{}",
        result.stderr()
    );
}

#[test]
fn cloud_status_against_mock_prints_viewer() {
    let runner = CodelRunner::new();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/me");
        then.status(200).json_body(json!({
            "user_id": "user_admin",
            "email": "admin@team.dev",
            "org": { "id": "org_acme", "name": "Acme" },
            "org_role": "admin"
        }));
    });

    let result = runner.run(&[
        "cloud",
        "status",
        "--api-url",
        &server.base_url(),
        "--token",
        "tok",
    ]);
    result.assert_success();
    let out = result.stdout();
    assert!(out.contains("user: user_admin"), "got:\n{out}");
    assert!(out.contains("Acme"), "got:\n{out}");
    assert!(out.contains("role: admin"), "got:\n{out}");
}

#[test]
fn cloud_push_sends_local_approved_memory() {
    let runner = CodelRunner::new();
    seed_approved(&runner, "mem-local", "Run cargo from core/.");

    let server = MockServer::start();
    let push = server.mock(|when, then| {
        when.method(POST).path("/projects/proj-cloud/memory");
        then.status(201).json_body(cloud_memory_json(
            "mem_remote",
            "Run cargo from core/.",
            "candidate",
        ));
    });

    let result = runner.run(&[
        "cloud",
        "push",
        "--api-url",
        &server.base_url(),
        "--token",
        "tok",
        "--project",
        "proj-cloud",
    ]);
    result.assert_success();
    push.assert();
    assert!(
        result.stdout().contains("pushed 1 memories"),
        "push should report one memory pushed; got:\n{}",
        result.stdout()
    );
}

#[test]
fn cloud_pull_imports_approved_memory() {
    let runner = CodelRunner::new();

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/projects/proj-cloud/memory")
            .query_param("status", "approved");
        then.status(200).json_body(json!([cloud_memory_json(
            "mem_team",
            "Deploy with the release script.",
            "approved"
        )]));
    });

    let result = runner.run(&[
        "cloud",
        "pull",
        "--api-url",
        &server.base_url(),
        "--token",
        "tok",
        "--project",
        "proj-cloud",
    ]);
    result.assert_success();
    assert!(
        result.stdout().contains("imported 1 approved memories"),
        "pull should report one memory imported; got:\n{}",
        result.stdout()
    );

    // The imported memory is now present and approved in the local store.
    let local = store(&runner);
    let approved = local
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Approved))
        .expect("list approved");
    assert!(
        approved
            .iter()
            .any(|record| record.entry().id() == "cloud-mem_team"),
        "the pulled memory should be imported into the local store"
    );
}

// ---------------------------------------------------------------------------
// update: hermetic "up to date" report (no network, no self-update)
// ---------------------------------------------------------------------------

#[test]
fn update_with_explicit_older_version_reports_up_to_date() {
    // Passing an explicit `--version` short-circuits the GitHub release lookup
    // entirely (no network). v0.0.1 is older than any real build, so the binary
    // reports "up to date" and installs nothing — the only way to exercise
    // `update` hermetically without a real release server.
    let runner = CodelRunner::new();
    let result = runner.run(&["update", "--version", "v0.0.1"]);
    result.assert_success();
    assert!(
        result.stdout().contains("up to date"),
        "update against an older explicit version should report up to date; got:\n{}",
        result.stdout()
    );
}

// NOTE: `update --check` (and bare `update`) call `fetch_latest_release()`, which
// makes a live GitHub request. That cannot be exercised hermetically here (the
// harness has no real release server to point at, and the URL is hard-coded), so
// it is intentionally not covered. The `--version`-pinned path above proves the
// command parses, compares versions, and reports status end-to-end offline.

// ---------------------------------------------------------------------------
// Helpers for the cloud memory-store scenarios.
// ---------------------------------------------------------------------------

fn project() -> ProjectRef {
    // Matches the runner's injected `--project-id project-1 --project-name codel00p`.
    ProjectRef::new("project-1", "codel00p")
}

fn store(runner: &CodelRunner) -> codel00p_memory::StorageBackedMemoryStore<SqliteStorage> {
    let storage = SqliteStorage::open(runner.memory_db()).expect("open sqlite storage");
    codel00p_memory::StorageBackedMemoryStore::new(
        StorageScope::project("org-1", "project-1"),
        storage,
    )
}

fn seed_approved(runner: &CodelRunner, id: &str, content: &str) {
    let mut store = store(runner);
    store
        .create_candidate(MemoryCandidateInput::new(
            id,
            project(),
            MemoryKind::Convention,
            content,
            MemorySource::turn(
                SessionId::from_static("session-cli"),
                TurnId::from_static("turn-cli"),
            ),
        ))
        .expect("create candidate");
    store
        .review(id, ReviewDecision::approve("local-reviewer"))
        .expect("approve");
}

fn cloud_memory_json(id: &str, content: &str, status: &str) -> Value {
    json!({
        "id": id,
        "project": { "id": "proj-cloud", "name": "codel00p" },
        "kind": "convention",
        "status": status,
        "content": content,
        "tags": ["team"]
    })
}

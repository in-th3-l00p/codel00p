//! Integration tests for the local agent registry lifecycle (#13 phase 1):
//! create / list / use / show, plus a proof that an active agent's home (and its
//! memory db) resolves under `<base>/agents/<name>/`.

use super::support::*;

use std::process::{Command, Output};

/// Run codel00p with `CODEL00P_HOME` pointed at `home` (the base), without the
/// `--memory-db`/org/project overrides so home resolution drives everything.
fn run_in_home(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

#[test]
fn agent_create_list_use_show_lifecycle() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();

    // create
    let out = run_in_home(
        base,
        &[
            "agent",
            "create",
            "researcher",
            "--description",
            "digs deep",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("Created agent `researcher`"));
    // the agent home + metadata exist under <base>/agents/researcher.
    let agent_home = base.join("agents").join("researcher");
    assert!(agent_home.join("agent.toml").is_file());
    assert!(agent_home.join("persona.md").is_file());
    assert!(agent_home.join("config.toml").is_file());

    // list shows it, with default present and active.
    let out = run_in_home(base, &["agent", "list"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let listing = stdout(&out);
    assert!(listing.contains("researcher"), "listing: {listing}");
    assert!(listing.contains("digs deep"), "listing: {listing}");
    assert!(
        listing.contains("* default"),
        "default active by default: {listing}"
    );

    // use sets the pointer.
    let out = run_in_home(base, &["agent", "use", "researcher"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("Now using agent `researcher`"));
    assert_eq!(
        std::fs::read_to_string(base.join("active_agent"))
            .unwrap()
            .trim(),
        "researcher"
    );

    // list now marks researcher active, not default.
    let out = run_in_home(base, &["agent", "list"]);
    let listing = stdout(&out);
    assert!(listing.contains("* researcher"), "listing: {listing}");

    // show prints the home path.
    let out = run_in_home(base, &["agent", "show", "researcher"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let shown = stdout(&out);
    assert!(
        shown.contains(agent_home.to_str().unwrap()),
        "shown: {shown}"
    );
    assert!(shown.contains("active:      true"), "shown: {shown}");

    // use --default clears it.
    let out = run_in_home(base, &["agent", "use", "--default"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(!base.join("active_agent").exists());
}

#[test]
fn unknown_agent_lists_available() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();
    run_in_home(base, &["agent", "create", "alpha"]);
    let out = run_in_home(base, &["agent", "use", "ghost"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("unknown agent: `ghost`"), "err: {err}");
    assert!(err.contains("alpha"), "err: {err}");
}

#[test]
fn active_agent_scopes_memory_under_agent_home() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();

    run_in_home(base, &["agent", "create", "scribe"]);
    run_in_home(base, &["agent", "use", "scribe"]);

    // Touch the memory store while the active agent is `scribe`. With no
    // --memory-db override, the db resolves under CODEL00P_HOME, which the
    // override repoints to <base>/agents/scribe; opening the store creates it.
    let out = run_in_home(base, &["memory", "list"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    // The agent's sqlite lives under its home, NOT the base home.
    let agent_db = base.join("agents").join("scribe").join("memory.sqlite");
    assert!(
        agent_db.is_file(),
        "agent memory db should exist at {}",
        agent_db.display()
    );
    assert!(
        !base.join("memory.sqlite").exists(),
        "base memory db must stay untouched while agent is active"
    );
}

#[test]
fn create_from_clones_persona_but_fresh_memory() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();

    run_in_home(
        base,
        &[
            "agent",
            "create",
            "mentor",
            "--persona",
            "# Persona: mentor\nwise\n",
        ],
    );
    run_in_home(base, &["agent", "use", "mentor"]);
    // give mentor a memory db (opening the store creates it under mentor's home).
    run_in_home(base, &["memory", "list"]);
    run_in_home(base, &["agent", "use", "--default"]);

    let out = run_in_home(base, &["agent", "create", "apprentice", "--from", "mentor"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    let apprentice = base.join("agents").join("apprentice");
    assert_eq!(
        std::fs::read_to_string(apprentice.join("persona.md")).unwrap(),
        "# Persona: mentor\nwise\n"
    );
    // fresh memory — the clone must not carry mentor's sqlite.
    assert!(!apprentice.join("memory.sqlite").exists());
}

//! Per-agent `config` + `skills`, and position-tolerant global flags (#13).
//!
//! An agent owns its own config, skills, memory, and sessions via the
//! `CODEL00P_HOME` boundary. These tests prove that `config` and `skills` — like
//! memory/sessions — honor the selected agent (`--agent <name>` or the sticky
//! pointer), and that the global flags work in ANY position (notably *after* the
//! subcommand, which used to error). They drive the REAL binary with a
//! `CODEL00P_HOME`-based runner (no `--memory-db`), so the home boundary alone
//! determines where config/skills resolve.

use super::support::*;

/// Run codel00p with `CODEL00P_HOME` = `home` (the base) and cwd = `home`, so the
/// home boundary resolves config/skills and no repo project-config bleeds in.
/// Injects NO global org/project/memory-db flags — `config`/`skills` don't need
/// the storage scope, and omitting `--memory-db` lets the agent home win.
fn run_home(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .current_dir(home)
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

/// Run via [`run_home`] and assert the command succeeded, returning its output.
fn run_ok(home: &Path, args: &[&str]) -> Output {
    let output = run_home(home, args);
    assert!(
        output.status.success(),
        "`{args:?}` failed: {}",
        stderr(&output)
    );
    output
}

/// `--agent` placed AFTER the subcommand is now accepted (it used to error
/// "unknown ... option: --agent") and targets the named agent's skills home.
#[test]
fn global_agent_flag_is_position_tolerant() {
    let home = tempdir().expect("tempdir");
    run_ok(home.path(), &["agent", "create", "coder"]);

    // Flag AFTER the subcommand token.
    let after = run_home(
        home.path(),
        &["skills", "--agent", "coder", "create", "demo-after"],
    );
    assert!(after.status.success(), "stderr: {}", stderr(&after));
    assert!(
        home.path()
            .join("agents/coder/skills/demo-after/SKILL.md")
            .is_file(),
        "skill should be created under the coder agent home"
    );

    // Flag in the LEADING position still works too.
    let before = run_home(
        home.path(),
        &["--agent", "coder", "skills", "create", "demo-before"],
    );
    assert!(before.status.success(), "stderr: {}", stderr(&before));
    assert!(
        home.path()
            .join("agents/coder/skills/demo-before/SKILL.md")
            .is_file(),
        "leading --agent should also target the coder home"
    );

    // Neither skill leaked into the base home.
    assert!(!home.path().join("skills/demo-after").exists());
    assert!(!home.path().join("skills/demo-before").exists());
}

/// Skills created under one agent are invisible to another agent and to the base
/// home — proving `skills` is agent-scoped via the home boundary.
#[test]
fn skills_isolate_per_agent() {
    let home = tempdir().expect("tempdir");
    run_ok(home.path(), &["agent", "create", "coder"]);
    run_ok(home.path(), &["agent", "create", "reviewer"]);

    run_ok(
        home.path(),
        &["--agent", "coder", "skills", "create", "coder-skill"],
    );
    run_ok(
        home.path(),
        &["--agent", "reviewer", "skills", "create", "reviewer-skill"],
    );

    let coder_list = run_home(home.path(), &["--agent", "coder", "skills", "list"]);
    assert!(
        coder_list.status.success(),
        "stderr: {}",
        stderr(&coder_list)
    );
    let coder_out = stdout(&coder_list);
    assert!(coder_out.contains("coder-skill"), "coder list: {coder_out}");
    assert!(
        !coder_out.contains("reviewer-skill"),
        "coder must not see reviewer's: {coder_out}"
    );

    let reviewer_list = run_home(home.path(), &["--agent", "reviewer", "skills", "list"]);
    let reviewer_out = stdout(&reviewer_list);
    assert!(
        reviewer_out.contains("reviewer-skill"),
        "reviewer list: {reviewer_out}"
    );
    assert!(
        !reviewer_out.contains("coder-skill"),
        "reviewer must not see coder's: {reviewer_out}"
    );

    // The base/default home sees neither agent's skills.
    let base_out = stdout(&run_home(home.path(), &["skills", "list"]));
    assert!(
        !base_out.contains("coder-skill"),
        "base saw coder's: {base_out}"
    );
    assert!(
        !base_out.contains("reviewer-skill"),
        "base saw reviewer's: {base_out}"
    );

    // On-disk: each agent's skill lives under its own home, not the other's.
    assert!(
        home.path()
            .join("agents/coder/skills/coder-skill/SKILL.md")
            .is_file()
    );
    assert!(
        !home
            .path()
            .join("agents/reviewer/skills/coder-skill")
            .exists()
    );
}

/// `config set/get` honor the selected agent: a value set under `coder` is read
/// back under `coder`, lands in coder's `config.toml`, and is absent from base.
#[test]
fn config_targets_the_selected_agent() {
    let home = tempdir().expect("tempdir");
    run_ok(home.path(), &["agent", "create", "coder"]);

    // Set on coder, with the flag MID-position to also exercise tolerance.
    let set = run_home(
        home.path(),
        &[
            "config",
            "--agent",
            "coder",
            "set",
            "agent.behavior.curator",
            "true",
        ],
    );
    assert!(set.status.success(), "stderr: {}", stderr(&set));

    // Coder reads back true.
    let coder_get = run_home(
        home.path(),
        &[
            "--agent",
            "coder",
            "config",
            "get",
            "agent.behavior.curator",
        ],
    );
    assert!(coder_get.status.success(), "stderr: {}", stderr(&coder_get));
    assert_eq!(stdout(&coder_get).trim(), "true");

    // The base/default home did NOT get the toggle (config_get prints empty when unset).
    let base_get = run_home(home.path(), &["config", "get", "agent.behavior.curator"]);
    assert!(base_get.status.success(), "stderr: {}", stderr(&base_get));
    assert!(
        stdout(&base_get).trim().is_empty(),
        "base config must not carry the agent's toggle, got: {:?}",
        stdout(&base_get)
    );

    // It physically lives in coder's config.toml.
    let coder_config = std::fs::read_to_string(home.path().join("agents/coder/config.toml"))
        .expect("read coder config.toml");
    assert!(
        coder_config.contains("curator = true"),
        "coder config.toml should carry the toggle, got:\n{coder_config}"
    );
}

/// The sticky active pointer (`agent use`) also scopes `config`/`skills`, not just
/// the one-shot `--agent` flag — proving consistency with memory/session scoping.
#[test]
fn sticky_active_agent_scopes_config_and_skills() {
    let home = tempdir().expect("tempdir");
    run_ok(home.path(), &["agent", "create", "coder"]);
    run_ok(home.path(), &["agent", "use", "coder"]);

    // No --agent flag: the sticky pointer should route these to coder.
    run_ok(home.path(), &["skills", "create", "sticky-skill"]);
    run_ok(
        home.path(),
        &["config", "set", "agent.behavior.curator", "true"],
    );

    assert!(
        home.path()
            .join("agents/coder/skills/sticky-skill/SKILL.md")
            .is_file(),
        "sticky agent should scope skills create to coder"
    );
    let coder_config = std::fs::read_to_string(home.path().join("agents/coder/config.toml"))
        .expect("read coder config.toml");
    assert!(
        coder_config.contains("curator = true"),
        "sticky agent should scope config to coder"
    );

    // The base home stayed clean.
    assert!(!home.path().join("skills/sticky-skill").exists());
}

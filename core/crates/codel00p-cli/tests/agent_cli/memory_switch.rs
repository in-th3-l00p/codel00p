//! Integration tests that PROVE the multi-agent memory switch (#13 phase 2).
//!
//! An agent is a `<base>/agents/<name>/` directory used as its own
//! `CODEL00P_HOME`. Switching agents = pointing the home at it, so each agent's
//! `memory.sqlite` and sessions isolate automatically via the home boundary.
//! These tests drive the REAL binary with NO `--memory-db` override, so home
//! resolution alone determines where memory/sessions land — exactly the path a
//! live switch takes. They assert:
//!   1. a memory created under agent `a` shows up under `a`;
//!   2. ISOLATION: switching to `b` shows none of `a`'s memory;
//!   3. PERSISTENCE: switching back to `a` still shows `a`'s memory;
//!   4. the base/default agent's memory is separate from both;
//!   5. sessions created under `a` are not listed under `b`.

use super::support::*;

use std::process::{Command, Output};

/// Run codel00p with `CODEL00P_HOME` = `home` (the base) and NO `--memory-db`
/// override, so the memory db + session store resolve under whichever agent home
/// the binary repoints to. The org/project flags keep the storage scope stable.
fn run_in_home(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .args(args)
        .output()
        .expect("run codel00p")
}

/// Deterministically create one memory candidate under the *active* agent by
/// importing a markdown file. `memory import` writes to the store resolved under
/// `CODEL00P_HOME`, which the home override repoints to the active agent. Returns
/// the unique marker so callers can assert presence/absence via `memory list`.
fn import_memory(home: &Path, marker: &str) -> String {
    // Write the source file under the base home dir (a stable, writable path).
    let src = home.join(format!("seed-{marker}.md"));
    std::fs::write(
        &src,
        format!("# {marker}\nThis memory belongs to the active agent: {marker}.\n"),
    )
    .expect("write seed md");
    let out = run_in_home(
        home,
        &[
            "memory",
            "import",
            src.to_str().unwrap(),
            "--kind",
            "convention",
        ],
    );
    assert!(out.status.success(), "import stderr: {}", stderr(&out));
    marker.to_string()
}

#[test]
fn memory_isolates_and_persists_across_agent_switches() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();

    // Two agents.
    assert!(
        run_in_home(base, &["agent", "create", "a"])
            .status
            .success(),
        "create a"
    );
    assert!(
        run_in_home(base, &["agent", "create", "b"])
            .status
            .success(),
        "create b"
    );

    // --- 1. Under agent `a`, create a memory; `a`'s list shows it. ---
    run_in_home(base, &["agent", "use", "a"]);
    import_memory(base, "AAA_memory");

    let list_a = run_in_home(base, &["memory", "list"]);
    assert!(
        list_a.status.success(),
        "list a stderr: {}",
        stderr(&list_a)
    );
    assert!(
        stdout(&list_a).contains("AAA_memory"),
        "agent a should see its own memory, got:\n{}",
        stdout(&list_a)
    );

    // The db physically lives under a's home, not the base.
    let a_db = base.join("agents").join("a").join("memory.sqlite");
    assert!(
        a_db.is_file(),
        "a's memory db should exist at {}",
        a_db.display()
    );

    // --- 2. ISOLATION: switch to `b`; b sees NONE of a's memory. ---
    run_in_home(base, &["agent", "use", "b"]);
    let list_b = run_in_home(base, &["memory", "list"]);
    assert!(
        list_b.status.success(),
        "list b stderr: {}",
        stderr(&list_b)
    );
    assert!(
        !stdout(&list_b).contains("AAA_memory"),
        "agent b must NOT see agent a's memory (isolation), got:\n{}",
        stdout(&list_b)
    );

    // Create b's own memory; it is visible under b but not under a.
    import_memory(base, "BBB_memory");
    let list_b2 = run_in_home(base, &["memory", "list"]);
    assert!(
        stdout(&list_b2).contains("BBB_memory"),
        "agent b should see its own memory, got:\n{}",
        stdout(&list_b2)
    );
    assert!(
        !stdout(&list_b2).contains("AAA_memory"),
        "agent b still must not see a's memory, got:\n{}",
        stdout(&list_b2)
    );

    // --- 3. PERSISTENCE across switch: back to `a`; a's memory is still there
    //        and a does NOT see b's memory. ---
    run_in_home(base, &["agent", "use", "a"]);
    let list_a2 = run_in_home(base, &["memory", "list"]);
    assert!(
        stdout(&list_a2).contains("AAA_memory"),
        "agent a's memory must persist across switches, got:\n{}",
        stdout(&list_a2)
    );
    assert!(
        !stdout(&list_a2).contains("BBB_memory"),
        "agent a must not see b's memory, got:\n{}",
        stdout(&list_a2)
    );

    // --- 4. DEFAULT isolation: the base/default agent's memory is separate. ---
    run_in_home(base, &["agent", "use", "--default"]);
    let list_default = run_in_home(base, &["memory", "list"]);
    assert!(
        list_default.status.success(),
        "list default stderr: {}",
        stderr(&list_default)
    );
    assert!(
        !stdout(&list_default).contains("AAA_memory")
            && !stdout(&list_default).contains("BBB_memory"),
        "the default agent must not see any agent's memory, got:\n{}",
        stdout(&list_default)
    );
    // Give the default agent its own memory and confirm the agents don't see it.
    import_memory(base, "DEF_memory");
    run_in_home(base, &["agent", "use", "a"]);
    let list_a3 = run_in_home(base, &["memory", "list"]);
    assert!(
        !stdout(&list_a3).contains("DEF_memory"),
        "agent a must not see the default agent's memory, got:\n{}",
        stdout(&list_a3)
    );

    // The three dbs are physically distinct files.
    assert!(
        base.join("agents")
            .join("a")
            .join("memory.sqlite")
            .is_file()
    );
    assert!(
        base.join("agents")
            .join("b")
            .join("memory.sqlite")
            .is_file()
    );
    assert!(
        base.join("memory.sqlite").is_file(),
        "the default agent's db lives at the base home"
    );
}

#[test]
fn learning_loop_writes_candidate_to_active_agent_home() {
    // The post-turn memory pipeline (extractor + recommender) writes candidates to
    // the store resolved under CODEL00P_HOME. With agent `a` active and NO
    // --memory-db override, that store IS `<base>/agents/a/memory.sqlite`, so a
    // candidate extracted during a's turn must land in a's home, not the base.
    let dir = tempdir().expect("tempdir");
    let base = dir.path();
    let workspace = base.join("workspace");
    std::fs::create_dir(&workspace).expect("create workspace");

    run_in_home(base, &["agent", "create", "a"]);
    run_in_home(base, &["agent", "use", "a"]);

    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Done.\nremember convention[learn]: AGENT_A_LEARNED_FACT applies here."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let out = run_in_home(
        base,
        &[
            "agent",
            "run",
            "Refactor the module.",
            "--workspace",
            workspace.to_str().unwrap(),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--permission-mode",
            "allow",
        ],
    );
    assert!(out.status.success(), "run stderr: {}", stderr(&out));

    // The candidate is listed under agent a (its own home's store).
    let list_a = run_in_home(base, &["memory", "list"]);
    assert!(
        stdout(&list_a).contains("AGENT_A_LEARNED_FACT"),
        "agent a's learning loop should write to a's memory, got:\n{}",
        stdout(&list_a)
    );

    // a's db exists; switching to the default agent shows none of a's learning.
    assert!(
        base.join("agents")
            .join("a")
            .join("memory.sqlite")
            .is_file()
    );
    run_in_home(base, &["agent", "use", "--default"]);
    let list_default = run_in_home(base, &["memory", "list"]);
    assert!(
        !stdout(&list_default).contains("AGENT_A_LEARNED_FACT"),
        "the default agent must not see agent a's learned memory, got:\n{}",
        stdout(&list_default)
    );
}

#[test]
fn sessions_isolate_per_agent() {
    let dir = tempdir().expect("tempdir");
    let base = dir.path();

    run_in_home(base, &["agent", "create", "a"]);
    run_in_home(base, &["agent", "create", "b"]);

    // Seed a session directly into agent a's store (under a's home). Sessions and
    // memory share one sqlite db resolved at `<home>/memory.sqlite` (see
    // `open_session_store`), so seed there via the same storage types the runtime
    // uses so the CLI lists it.
    run_in_home(base, &["agent", "use", "a"]);
    let a_home = base.join("agents").join("a");
    seed_chat_session(
        &a_home.join("memory.sqlite"),
        "session-a-only",
        &[SessionMessage::user("hello from a")],
    );

    // Under a, the session is listed.
    let list_a = run_in_home(base, &["session", "list"]);
    assert!(
        list_a.status.success(),
        "session list a: {}",
        stderr(&list_a)
    );
    assert!(
        stdout(&list_a).contains("session-a-only"),
        "agent a should list its own session, got:\n{}",
        stdout(&list_a)
    );

    // Switch to b: a's session must NOT appear (separate session store).
    run_in_home(base, &["agent", "use", "b"]);
    let list_b = run_in_home(base, &["session", "list"]);
    assert!(
        list_b.status.success(),
        "session list b: {}",
        stderr(&list_b)
    );
    assert!(
        !stdout(&list_b).contains("session-a-only"),
        "agent b must NOT see agent a's session (isolation), got:\n{}",
        stdout(&list_b)
    );
}

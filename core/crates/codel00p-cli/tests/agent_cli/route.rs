//! `agent route <task>` — description-based specialist selection (#13).

use super::support::*;

/// Run a base-scoped management command with `CODEL00P_HOME` = `home`.
fn run_in_home(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

fn seed_specialists(home: &Path) {
    let ok = |args: &[&str]| assert!(run_in_home(home, args).status.success());
    ok(&[
        "agent",
        "create",
        "coder",
        "--description",
        "implements features and refactors rust code",
    ]);
    ok(&[
        "agent",
        "create",
        "reviewer",
        "--description",
        "reviews pull requests for correctness",
    ]);
    ok(&[
        "agent",
        "create",
        "devops",
        "--description",
        "manages kubernetes deployments and cloud infrastructure",
    ]);
}

#[test]
fn route_picks_the_topical_specialist() {
    let dir = tempdir().expect("tempdir");
    seed_specialists(dir.path());

    let out = run_in_home(
        dir.path(),
        &["agent", "route", "refactor the rust parser code"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Best match: coder"), "routing output: {text}");
}

#[test]
fn route_json_reports_best_and_ranked_matches() {
    let dir = tempdir().expect("tempdir");
    seed_specialists(dir.path());

    let out = run_in_home(
        dir.path(),
        &[
            "agent",
            "route",
            "deploy the service to the kubernetes cluster",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let payload: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("route json");
    assert_eq!(payload["best"], "devops");
    let matches = payload["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 3, "all three agents ranked");
    // Ranked best-first: the top entry is the deployment specialist with the top score.
    assert_eq!(matches[0]["name"], "devops");
    assert!(matches[0]["score"].as_u64().unwrap() >= matches[1]["score"].as_u64().unwrap());
}

#[test]
fn route_limit_truncates_to_best_n() {
    let dir = tempdir().expect("tempdir");
    seed_specialists(dir.path());

    let out = run_in_home(
        dir.path(),
        &[
            "agent",
            "route",
            "review this code for bugs",
            "--json",
            "--limit",
            "1",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let payload: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("route json");
    assert_eq!(payload["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn route_reports_no_match_for_unrelated_task() {
    let dir = tempdir().expect("tempdir");
    seed_specialists(dir.path());

    let out = run_in_home(
        dir.path(),
        &[
            "agent",
            "route",
            "compose a haiku about the autumn moon",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let payload: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("route json");
    assert!(
        payload["best"].is_null(),
        "unrelated task should have no confident route: {payload}"
    );
}

#[test]
fn route_errors_without_agents() {
    let dir = tempdir().expect("tempdir");
    let out = run_in_home(dir.path(), &["agent", "route", "do something"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("no agents to route to"),
        "stderr: {}",
        stderr(&out)
    );
}

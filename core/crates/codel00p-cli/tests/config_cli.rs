use std::{
    path::Path,
    process::{Command, Output},
};

use httpmock::{Method::POST, MockServer};
use serde_json::json;
use tempfile::tempdir;

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .current_dir(home)
        .args(args)
        .output()
        .expect("run codel00p")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

#[test]
fn config_init_set_get_show_round_trip() {
    let home = tempdir().expect("tempdir");

    assert!(run(home.path(), &["config", "init"]).status.success());
    assert!(home.path().join("config.toml").exists());

    let set = run(
        home.path(),
        &["config", "set", "agent.provider", "openrouter"],
    );
    assert!(set.status.success(), "stderr: {}", stderr(&set));

    let get = run(home.path(), &["config", "get", "agent.provider"]);
    assert_eq!(stdout(&get).trim(), "openrouter");

    let show = run(home.path(), &["config", "show"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("provider:     openrouter"));
}

#[test]
fn config_set_rejects_unknown_key() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["config", "set", "agent.bogus", "x"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown config key"));
}

#[test]
fn config_get_defaults_to_show() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["config"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("codel00p configuration"));
}

#[test]
fn agent_run_uses_configured_provider_model_and_base_url() {
    let home = tempdir().expect("tempdir");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""model":"test-model""#)
            .body_includes("Hello from config.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "Configured reply." },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    assert!(
        run(home.path(), &["config", "set", "agent.provider", "custom"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["config", "set", "agent.model", "test-model"])
            .status
            .success()
    );
    assert!(
        run(
            home.path(),
            &["config", "set", "agent.base_url", &server.base_url()]
        )
        .status
        .success()
    );

    // No --provider / --model / --base-url flags: everything comes from config.
    let output = Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home.path())
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .current_dir(home.path())
        .args(["agent", "run", "Hello from config."])
        .output()
        .expect("run codel00p");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Configured reply.\n");
    provider.assert();
}

#[test]
fn config_profiles_list_shows_presets_and_user_profiles() {
    let home = tempdir().expect("tempdir");

    // A user profile (also shadowing the `careful` preset is not needed here).
    assert!(
        run(
            home.path(),
            &["config", "set", "agent.profiles.review.tool_sets", "read"],
        )
        .status
        .success()
    );
    assert!(
        run(
            home.path(),
            &[
                "config",
                "set",
                "agent.profiles.review.description",
                "Read-only review"
            ],
        )
        .status
        .success()
    );
    assert!(
        run(home.path(), &["config", "set", "agent.profile", "careful"])
            .status
            .success()
    );

    let list = run(home.path(), &["config", "profiles", "list"]);
    assert!(list.status.success(), "stderr: {}", stderr(&list));
    let text = stdout(&list);
    // Built-in presets are listed.
    assert!(text.contains("autonomous"), "{text}");
    assert!(text.contains("careful"), "{text}");
    assert!(text.contains("manual"), "{text}");
    // The user profile and its description appear.
    assert!(text.contains("review"), "{text}");
    assert!(text.contains("Read-only review"), "{text}");
    // The active default is surfaced.
    assert!(
        text.contains("active default (agent.profile): careful"),
        "{text}"
    );
}

#[test]
fn config_profiles_show_renders_preset_overrides() {
    let home = tempdir().expect("tempdir");
    let show = run(home.path(), &["config", "profiles", "show", "manual"]);
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    let text = stdout(&show);
    assert!(text.contains("manual"), "{text}");
    assert!(text.contains("built-in preset"), "{text}");
    // The manual preset turns autonomy off and restricts to read+edit.
    assert!(text.contains("auto_plan=false"), "{text}");
    assert!(text.contains("tool_sets=[read,edit]"), "{text}");
}

#[test]
fn config_profiles_show_unknown_errors_listing_available() {
    let home = tempdir().expect("tempdir");
    let show = run(home.path(), &["config", "profiles", "show", "nope"]);
    assert!(!show.status.success());
    let err = stderr(&show);
    assert!(err.contains("nope"), "{err}");
    assert!(err.contains("autonomous"), "{err}");
}

#[test]
fn agent_run_applies_configured_profile() {
    let home = tempdir().expect("tempdir");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        }));
    });

    for (key, value) in [
        ("agent.provider", "custom"),
        ("agent.model", "test-model"),
        ("agent.base_url", &server.base_url()[..]),
        // Select the `manual` preset as the default profile.
        ("agent.profile", "manual"),
    ] {
        assert!(
            run(home.path(), &["config", "set", key, value])
                .status
                .success()
        );
    }

    // The run succeeds with the profile applied (manual => read+edit only). We
    // assert it does not error out resolving the profile.
    let output = Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home.path())
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .current_dir(home.path())
        .args(["agent", "run", "hi"])
        .output()
        .expect("run codel00p");
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    provider.assert();
}

#[test]
fn agent_run_with_unknown_profile_errors() {
    let home = tempdir().expect("tempdir");
    for (key, value) in [("agent.provider", "custom"), ("agent.model", "m")] {
        assert!(
            run(home.path(), &["config", "set", key, value])
                .status
                .success()
        );
    }
    let output = run(home.path(), &["agent", "run", "hi", "--profile", "bogus"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("bogus"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn agent_run_without_model_reports_friendly_error() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["agent", "run", "hi", "--provider", "custom"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no model configured"),
        "stderr: {}",
        stderr(&output)
    );
}

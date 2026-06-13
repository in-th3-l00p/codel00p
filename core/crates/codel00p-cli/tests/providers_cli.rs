use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .env_remove("CODEL00P_PROVIDER_CUSTOM_API_KEY")
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
fn providers_use_sets_default_and_list_marks_it() {
    let home = tempdir().expect("tempdir");

    let used = run(
        home.path(),
        &[
            "config",
            "providers",
            "use",
            "custom",
            "--model",
            "test-model",
        ],
    );
    assert!(used.status.success(), "stderr: {}", stderr(&used));

    let model = run(home.path(), &["config", "get", "agent.model"]);
    assert_eq!(stdout(&model).trim(), "test-model");

    let list = run(home.path(), &["config", "providers", "list"]);
    let listing = stdout(&list);
    assert!(listing.contains("custom"), "listing: {listing}");
    assert!(listing.contains("(default)"), "listing: {listing}");
}

#[test]
fn providers_set_key_writes_env_and_show_reports_it() {
    let home = tempdir().expect("tempdir");

    let set = run(
        home.path(),
        &["config", "providers", "set-key", "custom", "sk-test-123"],
    );
    assert!(set.status.success(), "stderr: {}", stderr(&set));

    let env_contents = fs::read_to_string(home.path().join(".env")).expect("read .env");
    assert!(
        env_contents.contains("CODEL00P_PROVIDER_CUSTOM_API_KEY=sk-test-123"),
        ".env: {env_contents}"
    );

    let show = run(home.path(), &["config", "providers", "show", "custom"]);
    assert!(stdout(&show).contains("credential:   set via"));

    let removed = run(
        home.path(),
        &["config", "providers", "remove-key", "custom"],
    );
    assert!(removed.status.success());
    let show = run(home.path(), &["config", "providers", "show", "custom"]);
    assert!(stdout(&show).contains("credential:   missing"));
}

#[test]
fn providers_use_rejects_unknown_provider() {
    let home = tempdir().expect("tempdir");
    let output = run(
        home.path(),
        &["config", "providers", "use", "not-a-provider"],
    );
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown provider"));
}

#[test]
fn providers_list_is_the_default_subcommand() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["config", "providers"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Providers"));
}

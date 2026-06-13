use std::{
    path::Path,
    process::{Command, Output},
};

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
fn plugins_list_shows_builtin_catalog_disabled_by_default() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["config", "plugins", "list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("system-info"), "list output: {text}");
    assert!(text.contains("[ ] system-info"), "list output: {text}");
}

#[test]
fn plugins_enable_disable_round_trip() {
    let home = tempdir().expect("tempdir");

    let enable = run(home.path(), &["config", "plugins", "enable", "system-info"]);
    assert!(enable.status.success(), "stderr: {}", stderr(&enable));
    assert!(stdout(&enable).contains("Enabled plugin system-info"));

    // The enable is reflected in the catalog listing...
    let listed = run(home.path(), &["config", "plugins", "list"]);
    assert!(stdout(&listed).contains("[x] system-info"));

    // ...and is stored as a normal config value.
    let get = run(home.path(), &["config", "get", "plugins.enabled"]);
    assert_eq!(stdout(&get).trim(), "system-info");

    let disable = run(
        home.path(),
        &["config", "plugins", "disable", "system-info"],
    );
    assert!(disable.status.success(), "stderr: {}", stderr(&disable));
    assert!(stdout(&disable).contains("Disabled plugin system-info"));

    let listed_again = run(home.path(), &["config", "plugins", "list"]);
    assert!(stdout(&listed_again).contains("[ ] system-info"));
}

#[test]
fn plugins_enable_rejects_unknown_plugin() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["config", "plugins", "enable", "nope"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown plugin: nope"));
}

#[test]
fn plugins_enable_is_idempotent() {
    let home = tempdir().expect("tempdir");
    assert!(
        run(home.path(), &["config", "plugins", "enable", "system-info"])
            .status
            .success()
    );
    let again = run(home.path(), &["config", "plugins", "enable", "system-info"]);
    assert!(again.status.success());
    assert!(stdout(&again).contains("already enabled"));
}

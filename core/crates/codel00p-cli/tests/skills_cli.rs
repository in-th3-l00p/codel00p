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
fn skills_list_is_empty_by_default() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["skills", "list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No skills found"));
}

#[test]
fn skills_create_list_show_round_trip() {
    let home = tempdir().expect("tempdir");

    let create = run(home.path(), &["skills", "create", "deploy"]);
    assert!(create.status.success(), "stderr: {}", stderr(&create));
    assert!(stdout(&create).contains("Created skill deploy"));
    assert!(home.path().join("skills/deploy/SKILL.md").exists());

    let list = run(home.path(), &["skills", "list"]);
    let listed = stdout(&list);
    assert!(listed.contains("deploy"), "list: {listed}");
    assert!(listed.contains("[user]"), "list: {listed}");

    let show = run(home.path(), &["skills", "show", "deploy"]);
    let shown = stdout(&show);
    assert!(shown.contains("deploy (user)"), "show: {shown}");
    assert!(
        shown.contains("Describe when this skill applies"),
        "show: {shown}"
    );
}

#[test]
fn skills_create_rejects_duplicate() {
    let home = tempdir().expect("tempdir");
    assert!(
        run(home.path(), &["skills", "create", "dup"])
            .status
            .success()
    );
    let again = run(home.path(), &["skills", "create", "dup"]);
    assert!(!again.status.success());
    assert!(stderr(&again).contains("already exists"));
}

#[test]
fn skills_show_unknown_errors() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["skills", "show", "nope"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown skill: nope"));
}

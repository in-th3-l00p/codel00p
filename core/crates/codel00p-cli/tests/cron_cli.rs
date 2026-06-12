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
fn cron_list_is_empty_by_default() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["cron", "list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No scheduled jobs"));
}

#[test]
fn cron_add_list_show_remove_round_trip() {
    let home = tempdir().expect("tempdir");

    let add = run(home.path(), &["cron", "add", "30m", "Run", "the", "checks"]);
    assert!(add.status.success(), "stderr: {}", stderr(&add));
    assert!(stdout(&add).contains("Added cron-1 (every 30m)"));

    let list = run(home.path(), &["cron", "list"]);
    let listed = stdout(&list);
    assert!(listed.contains("cron-1"), "list: {listed}");
    assert!(listed.contains("every 30m"), "list: {listed}");

    let show = run(home.path(), &["cron", "show", "cron-1"]);
    let shown = stdout(&show);
    assert!(shown.contains("schedule:  every 30m"), "show: {shown}");
    assert!(shown.contains("Run the checks"), "show: {shown}");

    let disable = run(home.path(), &["cron", "disable", "cron-1"]);
    assert!(stdout(&disable).contains("Disabled cron-1"));

    let remove = run(home.path(), &["cron", "remove", "cron-1"]);
    assert!(stdout(&remove).contains("Removed cron-1"));
    assert!(stdout(&run(home.path(), &["cron", "list"])).contains("No scheduled jobs"));
}

#[test]
fn cron_add_rejects_bad_schedule() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["cron", "add", "soon", "do it"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("invalid schedule"));
}

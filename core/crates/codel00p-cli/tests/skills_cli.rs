use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

/// Write a skill file directly under the home's skills dir.
fn write_skill(home: &Path, name: &str, front_matter_extra: &str) {
    let dir = home.join("skills").join(name);
    fs::create_dir_all(&dir).expect("skill dir");
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: d\n{front_matter_extra}---\nbody\n"),
    )
    .expect("write skill");
}

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

#[test]
fn skills_curate_archives_only_stale_agent_skills() {
    let home = tempdir().expect("tempdir");
    write_skill(home.path(), "agent-skill", "created_by: agent\n");
    write_skill(home.path(), "human-skill", "");

    // Dry run (min-age 0 so the just-written skill qualifies) lists the agent one.
    let dry = run(home.path(), &["skills", "curate", "--min-age", "0"]);
    assert!(dry.status.success(), "stderr: {}", stderr(&dry));
    let listed = stdout(&dry);
    assert!(listed.contains("agent-skill"), "dry: {listed}");
    assert!(!listed.contains("human-skill"), "dry: {listed}");

    // Apply archives the agent skill (reversibly), leaving the human one.
    let apply = run(
        home.path(),
        &["skills", "curate", "--apply", "--min-age", "0"],
    );
    assert!(stdout(&apply).contains("Archived 1 skill(s): agent-skill"));
    assert!(
        home.path()
            .join("skills/.archive/agent-skill/SKILL.md")
            .exists()
    );

    let after = stdout(&run(home.path(), &["skills", "list"]));
    assert!(!after.contains("agent-skill"), "after: {after}");
    assert!(after.contains("human-skill"), "after: {after}");
}

#[test]
fn skills_curate_reports_nothing_when_clean() {
    let home = tempdir().expect("tempdir");
    write_skill(home.path(), "human-skill", "");
    let output = run(home.path(), &["skills", "curate", "--min-age", "0"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("No skills to curate"));
}

#[test]
fn skills_curate_consolidates_near_duplicate_agent_skills() {
    let home = tempdir().expect("tempdir");
    // Two agent skills with identical content → near-duplicates. The default
    // grace period keeps them out of the stale pass, so only consolidation fires.
    write_skill(home.path(), "a-deploy", "created_by: agent\n");
    write_skill(home.path(), "b-deploy", "created_by: agent\n");

    let dry = run(home.path(), &["skills", "curate"]);
    assert!(dry.status.success(), "stderr: {}", stderr(&dry));
    let listed = stdout(&dry);
    assert!(listed.contains("Near-duplicate agent skills"), "dry: {listed}");
    assert!(listed.contains("keep a-deploy"), "dry: {listed}");
    assert!(listed.contains("archive b-deploy"), "dry: {listed}");
    assert!(!listed.contains("Stale agent-created skills"), "dry: {listed}");

    let apply = run(home.path(), &["skills", "curate", "--apply"]);
    assert!(apply.status.success(), "stderr: {}", stderr(&apply));
    assert!(
        stdout(&apply).contains("Archived 1 skill(s): b-deploy"),
        "apply: {}",
        stdout(&apply)
    );
    assert!(
        home.path()
            .join("skills/.archive/b-deploy/SKILL.md")
            .exists()
    );

    let after = stdout(&run(home.path(), &["skills", "list"]));
    assert!(after.contains("a-deploy"), "survivor missing: {after}");
    assert!(!after.contains("b-deploy"), "duplicate still active: {after}");
}

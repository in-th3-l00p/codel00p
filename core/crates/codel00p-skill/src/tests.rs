//! Unit tests for skill loading, proposal review, and curation.

use super::*;
use std::fs;
use tempfile::tempdir;

fn write_skill(root: &Path, dir: &str, contents: &str) {
    let skill_dir = root.join(dir);
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(skill_dir.join(SKILL_FILE), contents).expect("write SKILL.md");
}

#[test]
fn parses_front_matter_and_body() {
    let dir = tempdir().expect("tempdir");
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\nversion: 1.2.0\ndescription: \"Ship the app\"\nauthor: ada\ntriggers:\n  - deploy\n  - release\n---\n# Deploy\n\nRun the deploy steps.\n",
    );

    let skill = load_skill(
        &dir.path().join("deploy").join(SKILL_FILE),
        SkillSource::User,
    )
    .expect("load");

    assert_eq!(skill.name, "deploy");
    assert_eq!(skill.version.as_deref(), Some("1.2.0"));
    assert_eq!(skill.description, "Ship the app");
    assert_eq!(skill.author.as_deref(), Some("ada"));
    assert_eq!(skill.triggers, vec!["deploy", "release"]);
    assert_eq!(skill.source, SkillSource::User);
    assert!(skill.body.starts_with("# Deploy"));
    assert!(skill.body.ends_with("Run the deploy steps."));
}

#[test]
fn supports_inline_trigger_lists_and_name_fallback() {
    let dir = tempdir().expect("tempdir");
    // No `name` field — falls back to the directory name.
    write_skill(
        dir.path(),
        "lint-fix",
        "---\ndescription: Fix lints\ntriggers: [lint, format]\n---\nBody.\n",
    );

    let skill = load_skill(
        &dir.path().join("lint-fix").join(SKILL_FILE),
        SkillSource::Project,
    )
    .expect("load");
    assert_eq!(skill.name, "lint-fix");
    assert_eq!(skill.triggers, vec!["lint", "format"]);
}

#[test]
fn missing_front_matter_is_an_error() {
    let dir = tempdir().expect("tempdir");
    write_skill(dir.path(), "bad", "no front matter here\n");
    let error =
        load_skill(&dir.path().join("bad").join(SKILL_FILE), SkillSource::User).unwrap_err();
    assert!(matches!(error, SkillError::MissingFrontMatter { .. }));
}

#[test]
fn selects_skills_by_trigger_relevance() {
    let dir = tempdir().expect("tempdir");
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: d\ntriggers: [deploy, ship]\n---\nbody\n",
    );
    write_skill(
        dir.path(),
        "lint",
        "---\nname: lint\ndescription: d\ntriggers: [lint]\n---\nbody\n",
    );
    let skills = load_skills(&[(SkillSource::User, dir.path().to_path_buf())]);

    let selected = select_skills(&skills, "please deploy the app", 5);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "deploy");

    // Unrelated query selects nothing.
    assert!(select_skills(&skills, "write a poem", 5).is_empty());

    // Limit is respected.
    let both = select_skills(&skills, "deploy and lint", 1);
    assert_eq!(both.len(), 1);
}

fn sample_proposal(name: &str) -> SkillProposal {
    SkillProposal {
        name: name.to_string(),
        description: "Ship the app safely".to_string(),
        triggers: vec!["deploy".to_string(), "release".to_string()],
        instructions: "1. Run tests.\n2. Deploy.\n3. Smoke test.".to_string(),
        created_by: "agent".to_string(),
    }
}

#[test]
fn proposed_skill_is_a_candidate_not_an_active_skill() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    propose_skill(root, &sample_proposal("deploy")).expect("propose");

    // Not visible as an active skill...
    assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());
    // ...but visible as a candidate, with provenance preserved.
    let candidates = load_candidates(root);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "deploy");
    assert_eq!(candidates[0].created_by.as_deref(), Some("agent"));
    assert_eq!(candidates[0].triggers, vec!["deploy", "release"]);
}

#[test]
fn proposal_is_rejected_when_name_is_taken() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    propose_skill(root, &sample_proposal("deploy")).expect("first propose");
    // A second proposal of the same name is a duplicate candidate.
    assert!(matches!(
        propose_skill(root, &sample_proposal("deploy")),
        Err(SkillError::CandidateExists { .. })
    ));

    // And an active skill blocks proposing the same name.
    write_skill(
        root,
        "active",
        "---\nname: active\ndescription: d\n---\nbody\n",
    );
    assert!(matches!(
        propose_skill(root, &sample_proposal("active")),
        Err(SkillError::AlreadyActive { .. })
    ));
}

#[test]
fn proposal_rejects_unsafe_names() {
    let dir = tempdir().expect("tempdir");
    assert!(matches!(
        propose_skill(dir.path(), &sample_proposal("../escape")),
        Err(SkillError::InvalidName { .. })
    ));
}

#[test]
fn approving_a_candidate_activates_it() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    propose_skill(root, &sample_proposal("deploy")).expect("propose");

    approve_candidate(root, "deploy").expect("approve");

    // Now an active skill, no longer a candidate.
    let active = load_skills(&[(SkillSource::User, root.to_path_buf())]);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "deploy");
    assert!(load_candidates(root).is_empty());

    // Approving an unknown candidate errors.
    assert!(matches!(
        approve_candidate(root, "missing"),
        Err(SkillError::UnknownCandidate { .. })
    ));
}

#[test]
fn rejecting_a_candidate_archives_it() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    propose_skill(root, &sample_proposal("deploy")).expect("propose");

    reject_candidate(root, "deploy").expect("reject");

    // Gone from the review queue and never activated...
    assert!(load_candidates(root).is_empty());
    assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());
    // ...but preserved in the archive for recovery.
    assert!(
        candidates_root(root)
            .join(ARCHIVE_DIR)
            .join("deploy")
            .join(SKILL_FILE)
            .is_file()
    );
}

#[test]
fn archiving_and_restoring_a_skill() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    write_skill(
        root,
        "stale",
        "---\nname: stale\ndescription: d\ncreated_by: agent\n---\nbody\n",
    );

    // Active before archiving.
    assert_eq!(
        load_skills(&[(SkillSource::User, root.to_path_buf())]).len(),
        1
    );

    archive_skill(root, "stale").expect("archive");
    // No longer loaded, but preserved in .archive.
    assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());
    assert!(archive_root(root).join("stale").join(SKILL_FILE).is_file());

    restore_skill(root, "stale").expect("restore");
    assert_eq!(
        load_skills(&[(SkillSource::User, root.to_path_buf())]).len(),
        1
    );

    assert!(matches!(
        archive_skill(root, "missing"),
        Err(SkillError::UnknownSkill { .. })
    ));
}

#[test]
fn load_archived_lists_disabled_skills() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    write_skill(
        root,
        "stale",
        "---\nname: stale\ndescription: d\ncreated_by: agent\n---\nbody\n",
    );

    // Nothing archived yet.
    assert!(load_archived(root).is_empty());

    archive_skill(root, "stale").expect("archive");
    let archived = load_archived(root);
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].name, "stale");
    // Archived skills are not part of the active set.
    assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());

    // Restoring removes it from the archive listing.
    restore_skill(root, "stale").expect("restore");
    assert!(load_archived(root).is_empty());
}

#[test]
fn curatable_only_targets_unused_old_agent_skills() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    write_skill(
        root,
        "agent-skill",
        "---\nname: agent-skill\ndescription: d\ncreated_by: agent\n---\nbody\n",
    );
    write_skill(
        root,
        "human-skill",
        "---\nname: human-skill\ndescription: d\n---\nbody\n",
    );
    let skills = load_skills(&[(SkillSource::User, root.to_path_buf())]);
    let agent = skills.iter().find(|s| s.name == "agent-skill").unwrap();
    let human = skills.iter().find(|s| s.name == "human-skill").unwrap();

    let unused = SkillUsage::default();
    let used = SkillUsage {
        count: 3,
        last_used_epoch: Some(5),
    };

    // Agent-created, unused, old enough -> curatable.
    assert!(is_curatable(agent, unused, 100, 50));
    // Too new (within the grace period) -> not yet.
    assert!(!is_curatable(agent, unused, 10, 50));
    // Used -> keep it.
    assert!(!is_curatable(agent, used, 100, 50));
    // Human-authored -> never curatable.
    assert!(!is_curatable(human, unused, 100, 50));
}

#[test]
fn project_skills_override_user_skills_by_name() {
    let user = tempdir().expect("user dir");
    let project = tempdir().expect("project dir");
    write_skill(
        user.path(),
        "deploy",
        "---\nname: deploy\ndescription: user version\n---\nuser body\n",
    );
    write_skill(
        project.path(),
        "deploy",
        "---\nname: deploy\ndescription: project version\n---\nproject body\n",
    );
    write_skill(
        user.path(),
        "test",
        "---\nname: test\ndescription: only in user\n---\nbody\n",
    );

    let skills = load_skills(&[
        (SkillSource::User, user.path().to_path_buf()),
        (SkillSource::Project, project.path().to_path_buf()),
    ]);

    // Sorted by name: deploy (project wins) then test (user only).
    assert_eq!(skills.len(), 2);
    let deploy = skills.iter().find(|s| s.name == "deploy").unwrap();
    assert_eq!(deploy.description, "project version");
    assert_eq!(deploy.source, SkillSource::Project);
    let test = skills.iter().find(|s| s.name == "test").unwrap();
    assert_eq!(test.source, SkillSource::User);
}

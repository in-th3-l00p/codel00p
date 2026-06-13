use super::support::*;

#[test]
fn agent_run_injects_relevant_skill_into_the_request() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    // Author a skill in the user skills dir (CODEL00P_HOME/skills).
    let skill_dir = dir.path().join("skills").join("deploy");
    fs::create_dir_all(&skill_dir).expect("skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy\ndescription: how to deploy\ntriggers:\n  - deploy\n---\nAlways run the smoke tests after deploying.\n",
    )
    .expect("write skill");

    let server = MockServer::start();
    // The mock only matches if the skill body was injected into the request.
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Always run the smoke tests after deploying.");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "done" }, "finish_reason": "stop" }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Please deploy the service",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    provider.assert();
}

#[test]
fn agent_proposes_skill_then_review_activates_it() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    // First call: the model proposes a skill via the propose_skill tool.
    let propose = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"propose_skill""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-learn",
                                "type": "function",
                                "function": {
                                    "name": "propose_skill",
                                    "arguments": "{\"name\":\"deploy-skill\",\"description\":\"How to deploy\",\"triggers\":[\"deploy\"],\"instructions\":\"Run tests then deploy.\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        }));
    });
    // Second call: after the proposal is recorded, the model wraps up.
    let finish = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("proposed");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "Proposed a skill." }, "finish_reason": "stop" }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Learn how to deploy this service",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "learn",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    propose.assert();
    finish.assert();

    // The proposal is a review candidate, not yet active.
    let candidate = dir.path().join("skills/.candidates/deploy-skill/SKILL.md");
    assert!(candidate.exists(), "candidate file should exist");

    let candidates = run_codel00p(&db_path, &["skills", "candidates"]);
    assert!(
        stdout(&candidates).contains("deploy-skill"),
        "candidates: {}",
        stdout(&candidates)
    );

    let list_before = run_codel00p(&db_path, &["skills", "list"]);
    assert!(
        !stdout(&list_before).contains("deploy-skill"),
        "candidate must not be active before approval"
    );

    // Approve it; now it is an active skill.
    let approve = run_codel00p(&db_path, &["skills", "approve", "deploy-skill"]);
    assert!(approve.status.success(), "stderr: {}", stderr(&approve));
    assert!(stdout(&approve).contains("Approved skill deploy-skill"));

    let list_after = run_codel00p(&db_path, &["skills", "list"]);
    assert!(
        stdout(&list_after).contains("deploy-skill"),
        "approved skill should be active: {}",
        stdout(&list_after)
    );
}

#[test]
fn agent_run_records_skill_usage() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let skill_dir = dir.path().join("skills").join("deploy");
    fs::create_dir_all(&skill_dir).expect("skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy\ndescription: how to deploy\ntriggers:\n  - deploy\n---\nShip carefully.\n",
    )
    .expect("write skill");

    let server = MockServer::start();
    let _provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }
            ]
        }));
    });

    // Before the run, the skill is unused.
    assert!(stdout(&run_codel00p(&db_path, &["skills", "list"])).contains("unused"));

    let run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Please deploy the service",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );
    assert!(run.status.success(), "stderr: {}", stderr(&run));

    // The injected skill's usage is now recorded.
    let listed = stdout(&run_codel00p(&db_path, &["skills", "list"]));
    assert!(listed.contains("deploy"), "list: {listed}");
    assert!(listed.contains("used 1x"), "list: {listed}");
}

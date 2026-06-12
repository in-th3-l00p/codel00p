use std::{
    path::Path,
    process::{Command, Output},
};

use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
    StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};
use codel00p_storage::{SqliteStorage, StorageScope};
use httpmock::prelude::*;
use serde_json::json;
use tempfile::tempdir;

fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

fn store(db_path: &Path) -> StorageBackedMemoryStore<SqliteStorage> {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage)
}

fn seed_approved(db_path: &Path, id: &str, content: &str) {
    let mut store = store(db_path);
    store
        .create_candidate(MemoryCandidateInput::new(
            id,
            project(),
            MemoryKind::Convention,
            content,
            MemorySource::turn(
                SessionId::from_static("session-cli"),
                TurnId::from_static("turn-cli"),
            ),
        ))
        .expect("create candidate");
    store
        .review(id, ReviewDecision::approve("local-reviewer"))
        .expect("approve");
}

fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", db_path.parent().unwrap_or(db_path))
        .arg("--memory-db")
        .arg(db_path)
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn cloud_memory_json(id: &str, content: &str, status: &str) -> serde_json::Value {
    json!({
        "id": id,
        "project": { "id": "proj-cloud", "name": "codel00p" },
        "kind": "convention",
        "status": status,
        "content": content,
        "tags": ["team"]
    })
}

#[test]
fn cloud_status_prints_viewer() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/me");
        then.status(200).json_body(json!({
            "user_id": "user_admin",
            "email": "admin@team.dev",
            "org": { "id": "org_acme", "name": "Acme" },
            "org_role": "admin"
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "status",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
        ],
    );

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let text = stdout(&output);
    assert!(text.contains("user: user_admin"));
    assert!(text.contains("Acme"));
    assert!(text.contains("role: admin"));
}

#[test]
fn cloud_push_sends_local_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_approved(&db_path, "mem-local", "Run cargo from core/.");

    let server = MockServer::start();
    let push = server.mock(|when, then| {
        when.method(POST).path("/projects/proj-cloud/memory");
        then.status(201).json_body(cloud_memory_json(
            "mem_remote",
            "Run cargo from core/.",
            "candidate",
        ));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "push",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
            "--project",
            "proj-cloud",
        ],
    );

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    push.assert();
    assert!(stdout(&output).contains("pushed 1 memories"));
}

#[test]
fn cloud_push_dry_run_makes_no_requests() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_approved(&db_path, "mem-local", "Run cargo from core/.");

    let server = MockServer::start();
    let push = server.mock(|when, then| {
        when.method(POST).path("/projects/proj-cloud/memory");
        then.status(201)
            .json_body(cloud_memory_json("mem_remote", "x", "candidate"));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "push",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
            "--project",
            "proj-cloud",
            "--dry-run",
        ],
    );

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(push.calls(), 0);
    assert!(stdout(&output).contains("dry run: 1 memories would be pushed"));
}

#[test]
fn cloud_pull_imports_approved_memory_into_local_store() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/projects/proj-cloud/memory")
            .query_param("status", "approved");
        then.status(200).json_body(json!([cloud_memory_json(
            "mem_team",
            "Deploy with the release script.",
            "approved"
        )]));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "pull",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
            "--project",
            "proj-cloud",
        ],
    );

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert!(stdout(&output).contains("imported 1 approved memories"));

    // The imported memory is now approved in the local store.
    let local = store(&db_path);
    let approved = local
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Approved))
        .expect("list approved");
    let imported = approved
        .iter()
        .find(|record| record.entry().id() == "cloud-mem_team")
        .expect("imported memory present");
    assert_eq!(
        imported.entry().content(),
        "Deploy with the release script."
    );

    // A second pull is idempotent — nothing new is imported.
    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "pull",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
            "--project",
            "proj-cloud",
        ],
    );
    assert!(stdout(&output).contains("imported 0 approved memories, skipped 1"));
}

#[test]
fn cloud_requires_connection_details() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let output = run_codel00p(&db_path, &["cloud", "status"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(stderr.contains("--api-url"), "stderr: {stderr}");
}

#[test]
fn cloud_uses_stored_login_credentials() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/me");
        then.status(200).json_body(json!({
            "user_id": "user_admin",
            "email": "admin@team.dev",
            "org": { "id": "org_acme", "name": "Acme" },
            "org_role": "admin"
        }));
    });

    // Simulate what `codel00p login` writes (CODEL00P_HOME = db_path.parent()).
    std::fs::write(
        dir.path().join("credentials.toml"),
        format!(
            "token = \"stored-token\"\napi_url = \"{}\"\n",
            server.base_url()
        ),
    )
    .expect("write credentials");

    // No --api-url / --token: the command reads stored credentials.
    let output = run_codel00p(&db_path, &["cloud", "status"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert!(stdout(&output).contains("user: user_admin"));
}

#[test]
fn cloud_run_resolves_agent_into_a_plan() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let server = MockServer::start();

    // The stored agent references mcp_1 only.
    server.mock(|when, then| {
        when.method(GET).path("/projects/proj-cloud/agents/agent_1");
        then.status(200).json_body(json!({
            "id": "agent_1",
            "org_id": "org_acme",
            "project_id": "proj-cloud",
            "name": "Release reviewer",
            "instructions": "Review the release.",
            "provider": "anthropic",
            "model": "claude-opus-4-8",
            "mcp_server_ids": ["mcp_1"],
            "created_by": "user_admin"
        }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/projects/proj-cloud/mcp-servers");
        then.status(200).json_body(json!([
            {
                "id": "mcp_1",
                "org_id": "org_acme",
                "project_id": "proj-cloud",
                "name": "GitHub",
                "transport": "http",
                "url": "https://mcp.example/sse",
                "enabled": true,
                "created_by": "user_admin"
            },
            {
                "id": "mcp_2",
                "org_id": "org_acme",
                "project_id": "proj-cloud",
                "name": "Unused",
                "transport": "stdio",
                "command": "x",
                "enabled": true,
                "created_by": "user_admin"
            }
        ]));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/projects/proj-cloud/memory/search")
            .query_param("q", "ship the release");
        then.status(200).json_body(json!([cloud_memory_json(
            "mem_1",
            "Deploy with the release script.",
            "approved"
        )]));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "cloud",
            "run",
            "agent_1",
            "--api-url",
            &server.base_url(),
            "--token",
            "tok",
            "--project",
            "proj-cloud",
            "--task",
            "ship the release",
            "--plan",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let plan: serde_json::Value = serde_json::from_str(stdout(&output).trim()).expect("plan json");

    assert_eq!(plan["agent"]["name"], "Release reviewer");
    assert_eq!(plan["agent"]["provider"], "anthropic");
    // Only the referenced MCP server is included.
    assert_eq!(plan["mcp_servers"].as_array().expect("array").len(), 1);
    assert_eq!(plan["mcp_servers"][0]["name"], "GitHub");
    assert_eq!(plan["context"].as_array().expect("array").len(), 1);
    let prompt = plan["system_prompt"].as_str().expect("prompt");
    assert!(prompt.contains("Review the release."));
    assert!(prompt.contains("Deploy with the release script."));
    assert!(prompt.contains("GitHub"));
}

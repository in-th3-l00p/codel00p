//! End-to-end tests for the cloud service over an in-memory backend: a real axum
//! server on an ephemeral port, driven by `reqwest` with RS256 session tokens.

mod common;

use codel00p_cloud::AppState;
use common::{admin_token, member_token, spawn, test_verifier};
use serde_json::Value;

async fn server() -> String {
    spawn(AppState::new(test_verifier())).await
}

#[tokio::test]
async fn healthz_is_public() {
    let base = server().await;
    let response = reqwest::get(format!("{base}/healthz"))
        .await
        .expect("request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn me_requires_a_valid_token() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client.get(format!("{base}/me")).send().await.expect("send");
    assert_eq!(response.status(), 401);

    let response = client
        .get(format!("{base}/me"))
        .bearer_auth("not-a-jwt")
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn me_returns_viewer_for_valid_session() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/me"))
        .bearer_auth(admin_token("org_acme"))
        .send()
        .await
        .expect("send");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["user_id"], "user_admin");
    assert_eq!(body["email"], "admin@team.dev");
    assert_eq!(body["org"]["id"], "org_acme");
    assert_eq!(body["org"]["slug"], "acme");
    assert_eq!(body["org_role"], "admin");
}

#[tokio::test]
async fn projects_create_and_list_round_trip() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/projects"))
        .bearer_auth(admin_token("org_acme"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body.as_array().expect("array").len(), 0);

    let response = client
        .post(format!("{base}/projects"))
        .bearer_auth(admin_token("org_acme"))
        .json(&serde_json::json!({
            "name": "Payments Service",
            "repository_url": "https://github.com/acme/payments"
        }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let created: Value = response.json().await.expect("json");
    assert_eq!(created["org_id"], "org_acme");
    assert_eq!(created["name"], "Payments Service");
    assert_eq!(created["slug"], "payments-service");

    let response = client
        .get(format!("{base}/projects"))
        .bearer_auth(admin_token("org_acme"))
        .send()
        .await
        .expect("send");
    let body: Value = response.json().await.expect("json");
    let projects = body.as_array().expect("array");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "Payments Service");
}

#[tokio::test]
async fn member_cannot_create_projects() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/projects"))
        .bearer_auth(member_token("org_acme"))
        .json(&serde_json::json!({ "name": "Forbidden" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 403);

    let response = client
        .get(format!("{base}/projects"))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    let body: Value = response.json().await.expect("json");
    assert_eq!(body.as_array().expect("array").len(), 0);
}

#[tokio::test]
async fn create_rejects_blank_name() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/projects"))
        .bearer_auth(admin_token("org_acme"))
        .json(&serde_json::json!({ "name": "   " }))
        .send()
        .await
        .expect("send");

    assert_eq!(response.status(), 400);
}

/// Creates a project as admin and returns its id.
async fn create_project(base: &str, client: &reqwest::Client) -> String {
    let response = client
        .post(format!("{base}/projects"))
        .bearer_auth(admin_token("org_acme"))
        .json(&serde_json::json!({ "name": "codel00p" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let created: Value = response.json().await.expect("json");
    created["id"].as_str().expect("id").to_string()
}

#[tokio::test]
async fn memory_review_queue_full_loop() {
    let base = server().await;
    let client = reqwest::Client::new();
    let project = create_project(&base, &client).await;

    // A member (e.g. a CLI agent) pushes a candidate.
    let response = client
        .post(format!("{base}/projects/{project}/memory"))
        .bearer_auth(member_token("org_acme"))
        .json(&serde_json::json!({
            "kind": "convention",
            "content": "Run cargo from core/.",
            "tags": ["testing"]
        }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let candidate: Value = response.json().await.expect("json");
    assert_eq!(candidate["status"], "candidate");
    let memory_id = candidate["id"].as_str().expect("id").to_string();

    // It shows up in the candidate queue.
    let response = client
        .get(format!("{base}/projects/{project}/memory?status=candidate"))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    let list: Value = response.json().await.expect("json");
    assert_eq!(list.as_array().expect("array").len(), 1);

    // A member cannot approve.
    let response = client
        .post(format!(
            "{base}/projects/{project}/memory/{memory_id}/approve"
        ))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 403);

    // An admin approves.
    let response = client
        .post(format!(
            "{base}/projects/{project}/memory/{memory_id}/approve"
        ))
        .bearer_auth(admin_token("org_acme"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.expect("json")["status"],
        "approved"
    );

    // Approved memory is now pullable (the "sync back" path).
    let response = client
        .get(format!("{base}/projects/{project}/memory?status=approved"))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    let approved: Value = response.json().await.expect("json");
    let approved = approved.as_array().expect("array");
    assert_eq!(approved.len(), 1);
    assert_eq!(approved[0]["content"], "Run cargo from core/.");

    // The audit trail records the creation and the approval.
    let response = client
        .get(format!(
            "{base}/projects/{project}/memory/{memory_id}/audit"
        ))
        .bearer_auth(admin_token("org_acme"))
        .send()
        .await
        .expect("send");
    let trail: Value = response.json().await.expect("json");
    let trail = trail.as_array().expect("array");
    assert_eq!(trail.len(), 2);
    assert_eq!(trail[0]["action"], "created");
    assert_eq!(trail[1]["action"], "approved");
    assert_eq!(trail[1]["actor"], "user_admin");
}

#[tokio::test]
async fn pushing_memory_to_unknown_project_is_not_found() {
    let base = server().await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/projects/proj_missing/memory"))
        .bearer_auth(member_token("org_acme"))
        .json(&serde_json::json!({ "kind": "decision", "content": "x" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn agents_crud_lifecycle() {
    let base = server().await;
    let client = reqwest::Client::new();
    let project = create_project(&base, &client).await;
    let admin = admin_token("org_acme");

    // Member cannot create.
    let response = client
        .post(format!("{base}/projects/{project}/agents"))
        .bearer_auth(member_token("org_acme"))
        .json(&serde_json::json!({ "name": "x", "provider": "anthropic", "model": "m" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 403);

    // Admin creates.
    let response = client
        .post(format!("{base}/projects/{project}/agents"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({
            "name": "Reviewer",
            "provider": "anthropic",
            "model": "claude-opus-4-8",
            "mcp_server_ids": ["mcp_1"]
        }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let agent: Value = response.json().await.expect("json");
    let agent_id = agent["id"].as_str().expect("id").to_string();
    assert_eq!(agent["created_by"], "user_admin");

    // Get + list.
    let response = client
        .get(format!("{base}/projects/{project}/agents/{agent_id}"))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
    let response = client
        .get(format!("{base}/projects/{project}/agents"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(
        response
            .json::<Value>()
            .await
            .expect("json")
            .as_array()
            .expect("array")
            .len(),
        1
    );

    // Update (PATCH).
    let response = client
        .patch(format!("{base}/projects/{project}/agents/{agent_id}"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "model": "claude-sonnet-4-6" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.expect("json")["model"],
        "claude-sonnet-4-6"
    );

    // Delete → 204, then 404.
    let response = client
        .delete(format!("{base}/projects/{project}/agents/{agent_id}"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 204);
    let response = client
        .get(format!("{base}/projects/{project}/agents/{agent_id}"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn mcp_servers_crud_and_validation() {
    let base = server().await;
    let client = reqwest::Client::new();
    let project = create_project(&base, &client).await;
    let admin = admin_token("org_acme");

    // http transport without url → 400.
    let response = client
        .post(format!("{base}/projects/{project}/mcp-servers"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "name": "GitHub", "transport": "http" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 400);

    // Valid create.
    let response = client
        .post(format!("{base}/projects/{project}/mcp-servers"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({
            "name": "GitHub",
            "transport": "http",
            "url": "https://mcp.example/sse"
        }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let server: Value = response.json().await.expect("json");
    let server_id = server["id"].as_str().expect("id").to_string();
    assert_eq!(server["enabled"], true);

    // Disable via PATCH.
    let response = client
        .patch(format!("{base}/projects/{project}/mcp-servers/{server_id}"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "enabled": false }))
        .send()
        .await
        .expect("send");
    assert_eq!(
        response.json::<Value>().await.expect("json")["enabled"],
        false
    );

    // Delete.
    let response = client
        .delete(format!("{base}/projects/{project}/mcp-servers/{server_id}"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 204);
}

#[tokio::test]
async fn project_update_and_delete() {
    let base = server().await;
    let client = reqwest::Client::new();
    let project = create_project(&base, &client).await;
    let admin = admin_token("org_acme");

    // Member cannot update.
    let response = client
        .patch(format!("{base}/projects/{project}"))
        .bearer_auth(member_token("org_acme"))
        .json(&serde_json::json!({ "name": "Renamed" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 403);

    // Admin updates.
    let response = client
        .patch(format!("{base}/projects/{project}"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "name": "Renamed" }))
        .send()
        .await
        .expect("send");
    assert_eq!(
        response.json::<Value>().await.expect("json")["name"],
        "Renamed"
    );

    // Delete → 204, then get → 404.
    let response = client
        .delete(format!("{base}/projects/{project}"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 204);
    let response = client
        .get(format!("{base}/projects/{project}"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn events_stream_pushes_changes_for_the_org() {
    use futures::StreamExt;
    use std::time::Duration;

    let base = server().await;
    let client = reqwest::Client::new();
    let admin = admin_token("org_acme");

    // Subscribe to the SSE stream first so the broadcast has a live subscriber.
    let response = client
        .get(format!("{base}/events"))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("connect events");
    assert_eq!(response.status(), 200);
    let mut stream = response.bytes_stream();

    // Trigger a change.
    create_project(&base, &client).await;

    // The change arrives on the stream.
    let mut buffer = String::new();
    let received = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&chunk.expect("chunk")));
            if buffer.contains("\"entity\":\"projects\"") {
                return true;
            }
        }
        false
    })
    .await
    .expect("timed out waiting for event");

    assert!(received, "expected a projects change event, got: {buffer}");
    assert!(buffer.contains("\"action\":\"created\""));
    assert!(buffer.contains("event:change") || buffer.contains("event: change"));
}

#[tokio::test]
async fn memory_search_returns_relevant_approved() {
    let base = server().await;
    let client = reqwest::Client::new();
    let project = create_project(&base, &client).await;
    let admin = admin_token("org_acme");

    // Push + approve a memory.
    let response = client
        .post(format!("{base}/projects/{project}/memory"))
        .bearer_auth(&admin)
        .json(&serde_json::json!({ "kind": "convention", "content": "Run cargo from the core directory" }))
        .send()
        .await
        .expect("send");
    let memory_id = response.json::<Value>().await.expect("json")["id"]
        .as_str()
        .expect("id")
        .to_string();
    client
        .post(format!(
            "{base}/projects/{project}/memory/{memory_id}/approve"
        ))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");

    // Search for it.
    let response = client
        .get(format!(
            "{base}/projects/{project}/memory/search?q=cargo+core"
        ))
        .bearer_auth(member_token("org_acme"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
    let hits: Value = response.json().await.expect("json");
    let hits = hits.as_array().expect("array");
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0]["content"]
            .as_str()
            .expect("content")
            .contains("cargo")
    );

    // A non-matching query returns nothing.
    let response = client
        .get(format!(
            "{base}/projects/{project}/memory/search?q=kubernetes"
        ))
        .bearer_auth(&admin)
        .send()
        .await
        .expect("send");
    assert_eq!(
        response
            .json::<Value>()
            .await
            .expect("json")
            .as_array()
            .expect("array")
            .len(),
        0
    );
}

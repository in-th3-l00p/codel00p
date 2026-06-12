//! Full-stack e2e against a real Postgres backend: HTTP → Clerk-style auth →
//! `spawn_blocking` → Postgres. Skipped unless `CODEL00P_TEST_DATABASE_URL` is
//! set, and requires the `postgres` feature.
#![cfg(feature = "postgres")]

mod common;

use std::sync::atomic::{AtomicU64, Ordering};

use codel00p_cloud::AppState;
use codel00p_storage::PostgresStorage;
use common::{admin_token, spawn, test_verifier};
use serde_json::{Value, json};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn database_url() -> Option<String> {
    std::env::var("CODEL00P_TEST_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// A unique org per run so create/list assertions are independent of prior runs.
fn unique_org() -> String {
    format!(
        "org-pg-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[tokio::test]
async fn postgres_backed_projects_round_trip() {
    let Some(url) = database_url() else {
        return;
    };

    // Connect off the async runtime: the blocking driver drives its own runtime.
    let storage = tokio::task::spawn_blocking(move || PostgresStorage::connect(&url))
        .await
        .expect("join")
        .expect("connect to postgres");
    let base = spawn(AppState::with_storage(Box::new(storage), test_verifier())).await;

    let org = unique_org();
    let token = admin_token(&org);
    let client = reqwest::Client::new();

    // Empty to start.
    let response = client
        .get(format!("{base}/projects"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 200);
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

    // Create through the full HTTP → auth → spawn_blocking → Postgres path.
    let response = client
        .post(format!("{base}/projects"))
        .bearer_auth(&token)
        .json(&json!({ "name": "Durable Project", "repository_url": "https://example.com/repo" }))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), 201);
    let created: Value = response.json().await.expect("json");
    assert_eq!(created["name"], "Durable Project");
    assert_eq!(created["org_id"], org);

    // Read it back from Postgres.
    let response = client
        .get(format!("{base}/projects"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("send");
    let projects = response.json::<Value>().await.expect("json");
    let projects = projects.as_array().expect("array");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "Durable Project");
    assert_eq!(projects[0]["repository_url"], "https://example.com/repo");
}

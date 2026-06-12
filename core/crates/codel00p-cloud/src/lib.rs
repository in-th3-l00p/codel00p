//! codel00p cloud service: the team control-plane HTTP API.
//!
//! The service is the source of truth for product data (projects, and later
//! provider policy, shared memory, and audit), reusing the engine crates and
//! persisting through `codel00p-storage`. Identity and membership come from
//! Clerk: every request carries a Clerk session JWT, verified at the boundary
//! and projected into an [`AuthContext`].

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use codel00p_protocol::{
    Agent, AgentUpdate, McpServer, McpServerUpdate, MemoryAuditEntry, MemoryEntry,
    MemoryReviewAction, MemoryStatus, NewAgent, NewMcpServer, NewMemoryCandidate, NewProject,
    Project, ProjectUpdate, Viewer,
};
use codel00p_storage::{InMemoryStorage, StorageBackend};
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// A live change notification broadcast to SSE subscribers in an organization.
#[derive(Clone, Debug, Serialize)]
pub struct ChangeEvent {
    pub org_id: String,
    pub entity: String,
    pub action: String,
}

impl ChangeEvent {
    fn new(org_id: impl Into<String>, entity: &str, action: &str) -> Self {
        Self {
            org_id: org_id.into(),
            entity: entity.to_string(),
            action: action.to_string(),
        }
    }
}

pub mod agents;
pub mod auth;
pub mod error;
pub mod mcp;
pub mod memory;
pub mod projects;

pub use auth::{AuthContext, JwtVerifier, VerifierError, clerk_frontend_api_from_publishable_key};
pub use error::ApiError;

/// Shared service state: durable storage plus the token verifier. Cheap to clone
/// (everything behind `Arc`), as axum requires. The storage is a trait object so
/// any backend — in-memory, SQLite, Postgres — works without touching handlers.
#[derive(Clone)]
pub struct AppState {
    storage: Arc<Mutex<Box<dyn StorageBackend>>>,
    verifier: Arc<JwtVerifier>,
    events: broadcast::Sender<ChangeEvent>,
}

impl AppState {
    /// Builds state over a fresh in-memory store.
    pub fn new(verifier: JwtVerifier) -> Self {
        Self::with_storage(Box::new(InMemoryStorage::default()), verifier)
    }

    /// Builds state over an explicit storage backend (e.g. Postgres).
    pub fn with_storage(storage: Box<dyn StorageBackend>, verifier: JwtVerifier) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            storage: Arc::new(Mutex::new(storage)),
            verifier: Arc::new(verifier),
            events,
        }
    }

    pub fn verifier(&self) -> &JwtVerifier {
        &self.verifier
    }

    /// Broadcasts a change to live subscribers. A send with no subscribers is a
    /// no-op, so callers never need to check.
    fn publish(&self, org_id: &str, entity: &str, action: &str) {
        let _ = self.events.send(ChangeEvent::new(org_id, entity, action));
    }

    /// Runs a storage operation on a blocking thread. The storage traits are
    /// synchronous (and a Postgres backend blocks), so they must not run on the
    /// async runtime's worker threads; `spawn_blocking` keeps the reactor free.
    pub async fn with_storage_blocking<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        F: FnOnce(&mut dyn StorageBackend) -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.storage.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = storage
                .lock()
                .map_err(|_| ApiError::Internal("storage lock poisoned".into()))?;
            f(&mut **guard)
        })
        .await
        .map_err(|error| ApiError::Internal(format!("storage task failed: {error}")))?
    }
}

/// Selects a storage backend from the environment. Returns `None` when no
/// `DATABASE_URL` is set, so the caller can fall back to in-memory storage.
/// Honouring `DATABASE_URL` requires the `postgres` feature.
pub fn storage_from_env()
-> Result<Option<Box<dyn StorageBackend>>, Box<dyn std::error::Error + Send + Sync>> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return Ok(None),
    };

    #[cfg(feature = "postgres")]
    {
        let storage = codel00p_storage::PostgresStorage::connect(&url)?;
        Ok(Some(Box::new(storage)))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = url;
        Err(
            "DATABASE_URL is set but codel00p-cloud was built without the `postgres` \
             feature; rebuild with `--features postgres`"
                .into(),
        )
    }
}

/// Builds the service router.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/me", get(me))
        .route("/events", get(events))
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{project_id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/projects/{project_id}/agents",
            get(list_agents).post(create_agent),
        )
        .route(
            "/projects/{project_id}/agents/{agent_id}",
            get(get_agent).patch(update_agent).delete(delete_agent),
        )
        .route(
            "/projects/{project_id}/mcp-servers",
            get(list_mcp).post(create_mcp),
        )
        .route(
            "/projects/{project_id}/mcp-servers/{server_id}",
            get(get_mcp).patch(update_mcp).delete(delete_mcp),
        )
        .route(
            "/projects/{project_id}/memory",
            get(list_memory).post(push_memory),
        )
        .route("/projects/{project_id}/memory/search", get(search_memory))
        .route(
            "/projects/{project_id}/memory/{memory_id}/approve",
            post(approve_memory),
        )
        .route(
            "/projects/{project_id}/memory/{memory_id}/reject",
            post(reject_memory),
        )
        .route(
            "/projects/{project_id}/memory/{memory_id}/audit",
            get(memory_audit),
        )
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn me(auth: AuthContext) -> Json<Viewer> {
    Json(auth.to_viewer())
}

/// `GET /events` — a Server-Sent Events stream of change notifications for the
/// caller's active organization. Clients subscribe to update the UI live instead
/// of polling.
async fn events(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let receiver = state.events.subscribe();

    let stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let org_id = org_id.clone();
        async move {
            match result {
                Ok(event) if event.org_id == org_id => {
                    let data = serde_json::to_string(&event).ok()?;
                    Some(Ok(Event::default().event("change").data(data)))
                }
                _ => None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn list_projects(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<Project>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();

    let projects = state
        .with_storage_blocking(move |store| projects::list_projects(&*store, &org_id))
        .await?;
    Ok(Json(projects))
}

async fn create_project(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(body): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org_id.clone();

    let project = state
        .with_storage_blocking(move |store| projects::create_project(store, &org_id, body))
        .await?;
    state.publish(&publish_org, "projects", "created");
    Ok((StatusCode::CREATED, Json(project)))
}

#[derive(Debug, Deserialize)]
struct MemoryQuery {
    status: Option<String>,
}

fn parse_status(value: &str) -> Result<MemoryStatus, ApiError> {
    serde_json::from_value(Value::String(value.to_string()))
        .map_err(|_| ApiError::BadRequest(format!("unknown memory status: {value}")))
}

async fn push_memory(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<NewMemoryCandidate>,
) -> Result<(StatusCode, Json<MemoryEntry>), ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let publish_org = org_id.clone();
    let actor = auth.user_id.clone();

    let entry = state
        .with_storage_blocking(move |store| {
            let project = projects::get_project(&*store, &org_id, &project_id)?
                .ok_or_else(|| ApiError::NotFound(format!("project {project_id} not found")))?;
            memory::push_candidate(store, &org_id, &project, body, &actor)
        })
        .await?;
    state.publish(&publish_org, "memory", "created");
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn list_memory(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(query): Query<MemoryQuery>,
) -> Result<Json<Vec<MemoryEntry>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let status = query.status.as_deref().map(parse_status).transpose()?;

    let entries = state
        .with_storage_blocking(move |store| {
            if projects::get_project(&*store, &org_id, &project_id)?.is_none() {
                return Err(ApiError::NotFound(format!(
                    "project {project_id} not found"
                )));
            }
            memory::list_memory(&*store, &org_id, &project_id, status)
        })
        .await?;
    Ok(Json(entries))
}

async fn approve_memory(
    state: State<AppState>,
    auth: AuthContext,
    path: Path<(String, String)>,
) -> Result<Json<MemoryEntry>, ApiError> {
    review_memory(state, auth, path, MemoryReviewAction::Approved).await
}

async fn reject_memory(
    state: State<AppState>,
    auth: AuthContext,
    path: Path<(String, String)>,
) -> Result<Json<MemoryEntry>, ApiError> {
    review_memory(state, auth, path, MemoryReviewAction::Rejected).await
}

async fn review_memory(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, memory_id)): Path<(String, String)>,
    action: MemoryReviewAction,
) -> Result<Json<MemoryEntry>, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org_id.clone();
    let actor = auth.user_id.clone();

    let entry = state
        .with_storage_blocking(move |store| {
            if projects::get_project(&*store, &org_id, &project_id)?.is_none() {
                return Err(ApiError::NotFound(format!(
                    "project {project_id} not found"
                )));
            }
            memory::review(store, &org_id, &project_id, &memory_id, action, &actor)
        })
        .await?;
    state.publish(&publish_org, "memory", "reviewed");
    Ok(Json(entry))
}

async fn memory_audit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, memory_id)): Path<(String, String)>,
) -> Result<Json<Vec<MemoryAuditEntry>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();

    let trail = state
        .with_storage_blocking(move |store| {
            memory::audit(&*store, &org_id, &project_id, &memory_id)
        })
        .await?;
    Ok(Json(trail))
}

#[derive(Debug, Deserialize)]
struct MemorySearchQuery {
    #[serde(default)]
    q: String,
    limit: Option<usize>,
}

/// Returns NotFound unless the project exists in the org. Used by nested
/// entity handlers so a bad project id is a 404 rather than an empty list.
fn require_project(
    store: &dyn StorageBackend,
    org_id: &str,
    project_id: &str,
) -> Result<(), ApiError> {
    if projects::get_project(store, org_id, project_id)?.is_none() {
        return Err(ApiError::NotFound(format!(
            "project {project_id} not found"
        )));
    }
    Ok(())
}

// --- Projects (get / update / delete) ---

async fn get_project(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let project = state
        .with_storage_blocking(move |store| {
            projects::get_project(&*store, &org_id, &project_id)?
                .ok_or_else(|| ApiError::NotFound(format!("project {project_id} not found")))
        })
        .await?;
    Ok(Json(project))
}

async fn update_project(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<ProjectUpdate>,
) -> Result<Json<Project>, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org_id.clone();
    let project = state
        .with_storage_blocking(move |store| {
            projects::update_project(store, &org_id, &project_id, body)
        })
        .await?;
    state.publish(&publish_org, "projects", "updated");
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org_id.clone();
    state
        .with_storage_blocking(move |store| projects::delete_project(store, &org_id, &project_id))
        .await?;
    state.publish(&publish_org, "projects", "deleted");
    Ok(StatusCode::NO_CONTENT)
}

// --- Agents ---

async fn list_agents(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<Agent>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let list = state
        .with_storage_blocking(move |store| {
            require_project(store, &org_id, &project_id)?;
            agents::list(&*store, &org_id, &project_id)
        })
        .await?;
    Ok(Json(list))
}

async fn create_agent(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<NewAgent>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    let actor = auth.user_id.clone();
    let agent = state
        .with_storage_blocking(move |store| {
            require_project(store, &org_id, &project_id)?;
            agents::create(store, &org_id, &project_id, body, &actor)
        })
        .await?;
    state.publish(&publish_org, "agents", "created");
    Ok((StatusCode::CREATED, Json(agent)))
}

async fn get_agent(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Agent>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let agent = state
        .with_storage_blocking(move |store| agents::get(&*store, &org_id, &project_id, &agent_id))
        .await?;
    Ok(Json(agent))
}

async fn update_agent(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentUpdate>,
) -> Result<Json<Agent>, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    let agent = state
        .with_storage_blocking(move |store| {
            agents::update(store, &org_id, &project_id, &agent_id, body)
        })
        .await?;
    state.publish(&publish_org, "agents", "updated");
    Ok(Json(agent))
}

async fn delete_agent(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    state
        .with_storage_blocking(move |store| agents::delete(store, &org_id, &project_id, &agent_id))
        .await?;
    state.publish(&publish_org, "agents", "deleted");
    Ok(StatusCode::NO_CONTENT)
}

// --- MCP servers ---

async fn list_mcp(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<McpServer>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let list = state
        .with_storage_blocking(move |store| {
            require_project(store, &org_id, &project_id)?;
            mcp::list(&*store, &org_id, &project_id)
        })
        .await?;
    Ok(Json(list))
}

async fn create_mcp(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<NewMcpServer>,
) -> Result<(StatusCode, Json<McpServer>), ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    let actor = auth.user_id.clone();
    let server = state
        .with_storage_blocking(move |store| {
            require_project(store, &org_id, &project_id)?;
            mcp::create(store, &org_id, &project_id, body, &actor)
        })
        .await?;
    state.publish(&publish_org, "mcp", "created");
    Ok((StatusCode::CREATED, Json(server)))
}

async fn get_mcp(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, server_id)): Path<(String, String)>,
) -> Result<Json<McpServer>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let server = state
        .with_storage_blocking(move |store| mcp::get(&*store, &org_id, &project_id, &server_id))
        .await?;
    Ok(Json(server))
}

async fn update_mcp(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, server_id)): Path<(String, String)>,
    Json(body): Json<McpServerUpdate>,
) -> Result<Json<McpServer>, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    let server = state
        .with_storage_blocking(move |store| {
            mcp::update(store, &org_id, &project_id, &server_id, body)
        })
        .await?;
    state.publish(&publish_org, "mcp", "updated");
    Ok(Json(server))
}

async fn delete_mcp(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, server_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let org = auth.require_org_admin()?;
    let org_id = org.id().to_string();
    let publish_org = org.id().to_string();
    state
        .with_storage_blocking(move |store| mcp::delete(store, &org_id, &project_id, &server_id))
        .await?;
    state.publish(&publish_org, "mcp", "deleted");
    Ok(StatusCode::NO_CONTENT)
}

// --- Memory RAG search ---

async fn search_memory(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(query): Query<MemorySearchQuery>,
) -> Result<Json<Vec<MemoryEntry>>, ApiError> {
    let (org, _role) = auth.require_org()?;
    let org_id = org.id().to_string();
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let hits = state
        .with_storage_blocking(move |store| {
            require_project(store, &org_id, &project_id)?;
            memory::search(&*store, &org_id, &project_id, &query.q, limit)
        })
        .await?;
    Ok(Json(hits))
}

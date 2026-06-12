use std::sync::atomic::{AtomicU64, Ordering};

use codel00p_protocol::{Agent, AgentUpdate, NewAgent};
use codel00p_storage::{DocumentStore, StorageDocument, StorageScope};

use crate::error::ApiError;

const COLLECTION: &str = "agents";
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn scope(org_id: &str, project_id: &str) -> StorageScope {
    StorageScope::project(org_id, project_id)
}

fn store_agent<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    agent: &Agent,
) -> Result<(), ApiError> {
    let payload = serde_json::to_value(agent).map_err(internal)?;
    let document = StorageDocument::new(scope(org_id, project_id), COLLECTION, agent.id(), payload);
    store.put_document(document).map_err(internal)?;
    Ok(())
}

/// Creates an agent in a project's shared pool.
pub fn create<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    request: NewAgent,
    actor: &str,
) -> Result<Agent, ApiError> {
    let name = request.name.trim();
    let provider = request.provider.trim();
    let model = request.model.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("agent name is required".into()));
    }
    if provider.is_empty() || model.is_empty() {
        return Err(ApiError::BadRequest(
            "agent provider and model are required".into(),
        ));
    }

    let id = format!("agent_{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut agent = Agent::new(&id, org_id, project_id, name, provider, model, actor)
        .with_mcp_server_ids(request.mcp_server_ids);
    if let Some(description) = request.description {
        agent = agent.with_description(description);
    }
    if let Some(instructions) = request.instructions {
        agent = agent.with_instructions(instructions);
    }

    store_agent(store, org_id, project_id, &agent)?;
    Ok(agent)
}

/// Lists a project's agents, ordered by id.
pub fn list<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
) -> Result<Vec<Agent>, ApiError> {
    let documents = store
        .list_documents(&scope(org_id, project_id), COLLECTION)
        .map_err(internal)?;
    documents
        .into_iter()
        .map(|document| {
            serde_json::from_value(document.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt agent record: {err}")))
        })
        .collect()
}

/// Fetches a single agent.
pub fn get<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
    agent_id: &str,
) -> Result<Agent, ApiError> {
    let document = store
        .get_document(&scope(org_id, project_id), COLLECTION, agent_id)
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound(format!("agent {agent_id} not found")))?;
    serde_json::from_value(document.payload().clone())
        .map_err(|err| ApiError::Internal(format!("corrupt agent record: {err}")))
}

/// Applies a partial update to an agent.
pub fn update<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    agent_id: &str,
    update: AgentUpdate,
) -> Result<Agent, ApiError> {
    let existing = get(store, org_id, project_id, agent_id)?;

    let name = update
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| existing.name().to_string());
    let provider = update
        .provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| existing.provider().to_string());
    let model = update
        .model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| existing.model().to_string());

    let mut agent = Agent::new(
        existing.id(),
        org_id,
        project_id,
        name,
        provider,
        model,
        existing.created_by(),
    )
    .with_mcp_server_ids(
        update
            .mcp_server_ids
            .unwrap_or_else(|| existing.mcp_server_ids().to_vec()),
    );
    if let Some(description) = update
        .description
        .or_else(|| existing.description().map(str::to_string))
    {
        agent = agent.with_description(description);
    }
    if let Some(instructions) = update
        .instructions
        .or_else(|| existing.instructions().map(str::to_string))
    {
        agent = agent.with_instructions(instructions);
    }

    store_agent(store, org_id, project_id, &agent)?;
    Ok(agent)
}

/// Deletes an agent. Returns NotFound if it does not exist.
pub fn delete<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    agent_id: &str,
) -> Result<(), ApiError> {
    let deleted = store
        .delete_document(&scope(org_id, project_id), COLLECTION, agent_id)
        .map_err(internal)?;
    if deleted {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!("agent {agent_id} not found")))
    }
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_storage::InMemoryStorage;

    fn new_agent() -> NewAgent {
        NewAgent {
            name: "Reviewer".into(),
            description: Some("Reviews PRs".into()),
            instructions: None,
            provider: "anthropic".into(),
            model: "claude-opus-4-8".into(),
            mcp_server_ids: vec!["mcp_1".into()],
        }
    }

    #[test]
    fn crud_round_trips() {
        let mut store = InMemoryStorage::default();
        let created =
            create(&mut store, "org_a", "proj_1", new_agent(), "user_admin").expect("create");
        assert_eq!(created.provider(), "anthropic");
        assert_eq!(created.mcp_server_ids(), &["mcp_1"]);

        let fetched = get(&store, "org_a", "proj_1", created.id()).expect("get");
        assert_eq!(fetched, created);

        let updated = update(
            &mut store,
            "org_a",
            "proj_1",
            created.id(),
            AgentUpdate {
                model: Some("claude-sonnet-4-6".into()),
                mcp_server_ids: Some(vec![]),
                ..AgentUpdate::default()
            },
        )
        .expect("update");
        assert_eq!(updated.model(), "claude-sonnet-4-6");
        assert!(updated.mcp_server_ids().is_empty());
        assert_eq!(updated.name(), "Reviewer"); // unchanged

        assert_eq!(list(&store, "org_a", "proj_1").expect("list").len(), 1);
        assert!(
            list(&store, "org_a", "proj_other")
                .expect("list other")
                .is_empty()
        );

        delete(&mut store, "org_a", "proj_1", created.id()).expect("delete");
        assert!(matches!(
            get(&store, "org_a", "proj_1", created.id()),
            Err(ApiError::NotFound(_))
        ));
    }

    #[test]
    fn create_validates_required_fields() {
        let mut store = InMemoryStorage::default();
        let blank_name = NewAgent {
            name: "  ".into(),
            ..new_agent()
        };
        assert!(matches!(
            create(&mut store, "org_a", "proj_1", blank_name, "u"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn delete_missing_is_not_found() {
        let mut store = InMemoryStorage::default();
        assert!(matches!(
            delete(&mut store, "org_a", "proj_1", "agent_x"),
            Err(ApiError::NotFound(_))
        ));
    }
}

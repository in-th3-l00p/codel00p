use std::sync::atomic::{AtomicU64, Ordering};

use codel00p_protocol::{McpServer, McpServerUpdate, McpTransport, NewMcpServer};
use codel00p_storage::{DocumentStore, StorageDocument, StorageScope};

use crate::error::ApiError;

const COLLECTION: &str = "mcp_servers";
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn scope(org_id: &str, project_id: &str) -> StorageScope {
    StorageScope::project(org_id, project_id)
}

fn store_server<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    server: &McpServer,
) -> Result<(), ApiError> {
    let payload = serde_json::to_value(server).map_err(internal)?;
    let document =
        StorageDocument::new(scope(org_id, project_id), COLLECTION, server.id(), payload);
    store.put_document(document).map_err(internal)?;
    Ok(())
}

/// Validates that an MCP server's transport has its required endpoint.
fn validate(
    transport: McpTransport,
    command: Option<&str>,
    url: Option<&str>,
) -> Result<(), ApiError> {
    match transport {
        McpTransport::Stdio if command.map(str::trim).unwrap_or("").is_empty() => Err(
            ApiError::BadRequest("stdio MCP servers require a command".into()),
        ),
        McpTransport::Http if url.map(str::trim).unwrap_or("").is_empty() => Err(
            ApiError::BadRequest("http MCP servers require a url".into()),
        ),
        _ => Ok(()),
    }
}

/// Creates an MCP server in a project's shared pool.
pub fn create<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    request: NewMcpServer,
    actor: &str,
) -> Result<McpServer, ApiError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("MCP server name is required".into()));
    }
    validate(
        request.transport,
        request.command.as_deref(),
        request.url.as_deref(),
    )?;

    let id = format!("mcp_{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut server = McpServer::new(&id, org_id, project_id, name, request.transport, actor)
        .with_enabled(request.enabled);
    if let Some(command) = request.command {
        server = server.with_command(command);
    }
    if let Some(url) = request.url {
        server = server.with_url(url);
    }

    store_server(store, org_id, project_id, &server)?;
    Ok(server)
}

pub fn list<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
) -> Result<Vec<McpServer>, ApiError> {
    let documents = store
        .list_documents(&scope(org_id, project_id), COLLECTION)
        .map_err(internal)?;
    documents
        .into_iter()
        .map(|document| {
            serde_json::from_value(document.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt mcp record: {err}")))
        })
        .collect()
}

pub fn get<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
    server_id: &str,
) -> Result<McpServer, ApiError> {
    let document = store
        .get_document(&scope(org_id, project_id), COLLECTION, server_id)
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound(format!("mcp server {server_id} not found")))?;
    serde_json::from_value(document.payload().clone())
        .map_err(|err| ApiError::Internal(format!("corrupt mcp record: {err}")))
}

pub fn update<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    server_id: &str,
    update: McpServerUpdate,
) -> Result<McpServer, ApiError> {
    let existing = get(store, org_id, project_id, server_id)?;

    let name = update
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| existing.name().to_string());
    let transport = update.transport.unwrap_or_else(|| existing.transport());
    let command = update
        .command
        .or_else(|| existing.command().map(str::to_string));
    let url = update.url.or_else(|| existing.url().map(str::to_string));
    let enabled = update.enabled.unwrap_or_else(|| existing.enabled());

    validate(transport, command.as_deref(), url.as_deref())?;

    let mut server = McpServer::new(
        existing.id(),
        org_id,
        project_id,
        name,
        transport,
        existing.created_by(),
    )
    .with_enabled(enabled);
    if let Some(command) = command {
        server = server.with_command(command);
    }
    if let Some(url) = url {
        server = server.with_url(url);
    }

    store_server(store, org_id, project_id, &server)?;
    Ok(server)
}

pub fn delete<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    server_id: &str,
) -> Result<(), ApiError> {
    let deleted = store
        .delete_document(&scope(org_id, project_id), COLLECTION, server_id)
        .map_err(internal)?;
    if deleted {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!(
            "mcp server {server_id} not found"
        )))
    }
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_storage::InMemoryStorage;

    #[test]
    fn crud_round_trips_and_validates_transport() {
        let mut store = InMemoryStorage::default();

        // http requires url.
        let missing_url = NewMcpServer {
            name: "GitHub".into(),
            transport: McpTransport::Http,
            command: None,
            url: None,
            enabled: true,
        };
        assert!(matches!(
            create(&mut store, "org_a", "proj_1", missing_url, "u"),
            Err(ApiError::BadRequest(_))
        ));

        let created = create(
            &mut store,
            "org_a",
            "proj_1",
            NewMcpServer {
                name: "GitHub".into(),
                transport: McpTransport::Http,
                command: None,
                url: Some("https://mcp.example/sse".into()),
                enabled: true,
            },
            "user_admin",
        )
        .expect("create");
        assert_eq!(created.transport(), McpTransport::Http);
        assert_eq!(created.url(), Some("https://mcp.example/sse"));

        let disabled = update(
            &mut store,
            "org_a",
            "proj_1",
            created.id(),
            McpServerUpdate {
                enabled: Some(false),
                ..McpServerUpdate::default()
            },
        )
        .expect("update");
        assert!(!disabled.enabled());

        assert_eq!(list(&store, "org_a", "proj_1").expect("list").len(), 1);
        delete(&mut store, "org_a", "proj_1", created.id()).expect("delete");
        assert!(matches!(
            delete(&mut store, "org_a", "proj_1", created.id()),
            Err(ApiError::NotFound(_))
        ));
    }
}

//! Blocking cloud fetches for the entity browser. Each fetch resolves stored
//! credentials, calls the typed `CloudClient`, and maps the result to a `Msg`. The
//! event loop runs these on `spawn_blocking` so the render loop never blocks.

use crate::cloud_client::CloudClient;
use crate::credentials;

use super::msg::{CloudFetch, Msg};

/// Whether the cloud is reachable: an API URL and token are available from env or
/// stored credentials. Used to decide if cloud panels should attempt a fetch.
pub(crate) fn cloud_configured() -> bool {
    resolve_connection().is_ok()
}

/// Runs one cloud fetch to completion and returns the corresponding `Msg`. Intended
/// to be called inside `tokio::task::spawn_blocking`.
pub(crate) fn run_cloud_fetch(fetch: CloudFetch) -> Msg {
    let client = match resolve_connection() {
        Ok((api_url, token)) => match CloudClient::new(&api_url, &token) {
            Ok(client) => client,
            Err(error) => return fetch_error(&fetch, error),
        },
        Err(error) => return fetch_error(&fetch, error),
    };

    match fetch {
        CloudFetch::Viewer => Msg::CloudViewer(client.viewer()),
        CloudFetch::Projects => Msg::CloudProjects(client.list_projects()),
        CloudFetch::Agents(project) => Msg::CloudAgents(client.list_agents(&project)),
        CloudFetch::Mcp(project) => Msg::CloudMcp(client.list_mcp_servers(&project)),
        CloudFetch::Memory(project) => {
            Msg::CloudMemory(client.list_memory(&project, Some("approved")))
        }
    }
}

/// Resolves `(api_url, token)` from env vars first, then stored credentials.
fn resolve_connection() -> Result<(String, String), String> {
    let stored = credentials::load();
    let api_url = std::env::var("CODEL00P_API_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(stored.api_url)
        .ok_or_else(|| "no cloud API URL — run `codel00p auth login`".to_string())?;
    let token = std::env::var("CODEL00P_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(stored.token)
        .ok_or_else(|| "not signed in — run `codel00p auth login`".to_string())?;
    Ok((api_url, token))
}

fn fetch_error(fetch: &CloudFetch, error: String) -> Msg {
    match fetch {
        CloudFetch::Viewer => Msg::CloudViewer(Err(error)),
        CloudFetch::Projects => Msg::CloudProjects(Err(error)),
        CloudFetch::Agents(_) => Msg::CloudAgents(Err(error)),
        CloudFetch::Mcp(_) => Msg::CloudMcp(Err(error)),
        CloudFetch::Memory(_) => Msg::CloudMemory(Err(error)),
    }
}

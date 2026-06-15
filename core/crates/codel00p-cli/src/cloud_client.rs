use codel00p_protocol::{
    Agent, McpServer, MemoryEntry, NewMemoryCandidate, OrgMember, Project, Viewer,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A blocking HTTP client for the codel00p cloud service, typed by the shared
/// protocol contracts and authenticated with a Clerk session token.
pub struct CloudClient {
    base_url: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl CloudClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .build()
            .map_err(|error| format!("failed to build http client: {error}"))?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            http,
        })
    }

    /// `GET /me` — the authenticated viewer and active organization.
    pub fn viewer(&self) -> Result<Viewer, String> {
        self.get("/me")
    }

    /// `GET /projects/{id}/memory` — optionally filtered by status.
    pub fn list_memory(
        &self,
        project_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<MemoryEntry>, String> {
        let mut path = format!("/projects/{project_id}/memory");
        if let Some(status) = status {
            path.push_str(&format!("?status={status}"));
        }
        self.get(&path)
    }

    /// `POST /projects/{id}/memory` — push a memory candidate.
    pub fn push_candidate(
        &self,
        project_id: &str,
        candidate: &NewMemoryCandidate,
    ) -> Result<MemoryEntry, String> {
        self.post(&format!("/projects/{project_id}/memory"), candidate)
    }

    /// `GET /projects` — the organization's projects (org scope from the token).
    pub fn list_projects(&self) -> Result<Vec<Project>, String> {
        self.get("/projects")
    }

    /// `GET /org/members` — the active organization's Clerk-backed roster.
    pub fn list_org_members(&self) -> Result<Vec<OrgMember>, String> {
        self.get("/org/members")
    }

    /// `GET /projects/{id}/agents` — the project's stored agent definitions.
    pub fn list_agents(&self, project_id: &str) -> Result<Vec<Agent>, String> {
        self.get(&format!("/projects/{project_id}/agents"))
    }

    /// `GET /projects/{id}/agents/{agent_id}` — a stored agent definition.
    pub fn get_agent(&self, project_id: &str, agent_id: &str) -> Result<Agent, String> {
        self.get(&format!("/projects/{project_id}/agents/{agent_id}"))
    }

    /// `GET /projects/{id}/mcp-servers` — the project's MCP server pool.
    pub fn list_mcp_servers(&self, project_id: &str) -> Result<Vec<McpServer>, String> {
        self.get(&format!("/projects/{project_id}/mcp-servers"))
    }

    /// `GET /projects/{id}/memory/search` — RAG retrieval over approved memory.
    pub fn search_memory(
        &self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let query = urlencode(query);
        self.get(&format!(
            "/projects/{project_id}/memory/search?q={query}&limit={limit}"
        ))
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .send()
            .map_err(|error| format!("request to {path} failed: {error}"))?;
        read_json(response)
    }

    fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T, String> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .map_err(|error| format!("request to {path} failed: {error}"))?;
        read_json(response)
    }
}

/// Minimal percent-encoding for a query-string value.
fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn read_json<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        // The service renders errors as { "error", "message" }; surface the
        // message when present, otherwise the raw body.
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or(body);
        return Err(format!("cloud request failed ({status}): {message}"));
    }
    response
        .json::<T>()
        .map_err(|error| format!("failed to decode cloud response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_protocol::MemoryKind;
    use httpmock::prelude::*;
    use serde_json::json;

    fn memory_entry_json(id: &str, status: &str) -> serde_json::Value {
        json!({
            "id": id,
            "project": { "id": "proj_1", "name": "codel00p" },
            "kind": "convention",
            "status": status,
            "content": "Run cargo from core/.",
            "tags": ["testing"]
        })
    }

    #[test]
    fn viewer_reads_me() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/me")
                .header("authorization", "Bearer tok");
            then.status(200)
                .json_body(json!({ "user_id": "user_1", "org_role": "admin" }));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let viewer = client.viewer().expect("viewer");

        mock.assert();
        assert_eq!(viewer.user_id(), "user_1");
    }

    #[test]
    fn list_memory_passes_status_filter() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/projects/proj_1/memory")
                .query_param("status", "approved");
            then.status(200)
                .json_body(json!([memory_entry_json("mem_1", "approved")]));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let entries = client
            .list_memory("proj_1", Some("approved"))
            .expect("list");

        mock.assert();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id(), "mem_1");
    }

    #[test]
    fn push_candidate_posts_body_and_returns_entry() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/proj_1/memory")
                .json_body(json!({
                    "kind": "convention",
                    "content": "Run cargo from core/.",
                    "tags": [],
                    "sensitivity": "normal"
                }));
            then.status(201)
                .json_body(memory_entry_json("mem_9", "candidate"));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let candidate = NewMemoryCandidate::new(MemoryKind::Convention, "Run cargo from core/.");
        let entry = client.push_candidate("proj_1", &candidate).expect("push");

        mock.assert();
        assert_eq!(entry.id(), "mem_9");
        assert_eq!(entry.status(), codel00p_protocol::MemoryStatus::Candidate);
    }

    #[test]
    fn non_success_surfaces_error_message() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/projects/proj_1/memory");
            then.status(403).json_body(
                json!({ "error": "forbidden", "message": "requires organization admin" }),
            );
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let error = client.list_memory("proj_1", None).unwrap_err();
        assert!(error.contains("403"));
        assert!(error.contains("requires organization admin"));
    }

    #[test]
    fn list_projects_and_agents() {
        let server = MockServer::start();
        let projects_mock = server.mock(|when, then| {
            when.method(GET).path("/projects");
            then.status(200).json_body(json!([{
                "id": "proj_1",
                "org_id": "org_a",
                "name": "codel00p",
                "slug": "codel00p"
            }]));
        });
        let agents_mock = server.mock(|when, then| {
            when.method(GET).path("/projects/proj_1/agents");
            then.status(200).json_body(json!([{
                "id": "agent_1",
                "org_id": "org_a",
                "project_id": "proj_1",
                "name": "Reviewer",
                "provider": "anthropic",
                "model": "claude-opus-4-8",
                "created_by": "user_admin"
            }]));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let projects = client.list_projects().expect("projects");
        let agents = client.list_agents("proj_1").expect("agents");

        projects_mock.assert();
        agents_mock.assert();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id(), "proj_1");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].model(), "claude-opus-4-8");
    }

    #[test]
    fn list_org_members_reads_roster() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/org/members")
                .header("authorization", "Bearer tok");
            then.status(200).json_body(json!([{
                "user_id": "user_1",
                "role": "admin",
                "email": "ada@example.com",
                "name": "Ada Lovelace"
            }]));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let members = client.list_org_members().expect("members");

        mock.assert();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].user_id(), "user_1");
        assert_eq!(members[0].email(), Some("ada@example.com"));
    }

    #[test]
    fn get_agent_and_search_memory() {
        let server = MockServer::start();
        let agent_mock = server.mock(|when, then| {
            when.method(GET).path("/projects/proj_1/agents/agent_1");
            then.status(200).json_body(json!({
                "id": "agent_1",
                "org_id": "org_a",
                "project_id": "proj_1",
                "name": "Reviewer",
                "provider": "anthropic",
                "model": "claude-opus-4-8",
                "created_by": "user_admin"
            }));
        });
        let search_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/projects/proj_1/memory/search")
                .query_param("q", "deploy")
                .query_param("limit", "5");
            then.status(200)
                .json_body(json!([memory_entry_json("mem_1", "approved")]));
        });

        let client = CloudClient::new(server.base_url(), "tok").expect("client");
        let agent = client.get_agent("proj_1", "agent_1").expect("agent");
        let hits = client.search_memory("proj_1", "deploy", 5).expect("search");

        agent_mock.assert();
        search_mock.assert();
        assert_eq!(agent.provider(), "anthropic");
        assert_eq!(hits.len(), 1);
    }
}

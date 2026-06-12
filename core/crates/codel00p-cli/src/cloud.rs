use std::env;

use codel00p_memory::{MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision};
use codel00p_protocol::{
    Agent, McpServer, MemoryEntry, MemorySource, MemoryStatus, NewMemoryCandidate,
};
use codel00p_providers::{ChatMessage, InferenceRequest, default_registry};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::cloud_client::CloudClient;
use crate::config::{CliConfig, CliResult, open_memory_store, required_value};
use crate::providers::build_provider_client_with;

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing cloud command".to_string());
    };

    match command.as_str() {
        "status" => cloud_status(rest),
        "push" => cloud_push(config, rest),
        "pull" => cloud_pull(config, rest),
        "run" => cloud_run(rest),
        _ => Err(format!("unknown cloud command: {command}")),
    }
}

/// A cloud agent resolved into everything needed to execute it: its definition,
/// the MCP servers it references, the RAG context for the task, and the assembled
/// system prompt.
struct RunPlan {
    agent: Agent,
    mcp_servers: Vec<McpServer>,
    context: Vec<MemoryEntry>,
    system_prompt: String,
}

/// Resolves a cloud agent into an executable [`RunPlan`]: fetch the agent, the
/// MCP servers it references, and (when a task is given) the relevant approved
/// memory, then assemble the system prompt.
fn resolve_plan(
    client: &CloudClient,
    project_id: &str,
    agent_id: &str,
    task: &str,
    limit: usize,
) -> CliResult<RunPlan> {
    let agent = client.get_agent(project_id, agent_id)?;

    let referenced: std::collections::HashSet<&str> =
        agent.mcp_server_ids().iter().map(String::as_str).collect();
    let mcp_servers: Vec<McpServer> = client
        .list_mcp_servers(project_id)?
        .into_iter()
        .filter(|server| referenced.contains(server.id()))
        .collect();

    let context = if task.trim().is_empty() {
        Vec::new()
    } else {
        client.search_memory(project_id, task, limit)?
    };

    let system_prompt = assemble_system_prompt(&agent, &mcp_servers, &context);

    Ok(RunPlan {
        agent,
        mcp_servers,
        context,
        system_prompt,
    })
}

fn assemble_system_prompt(
    agent: &Agent,
    mcp_servers: &[McpServer],
    context: &[MemoryEntry],
) -> String {
    let mut prompt = agent
        .instructions()
        .map(str::to_string)
        .unwrap_or_else(|| format!("You are {}, a codel00p agent.", agent.name()));

    if !context.is_empty() {
        prompt.push_str("\n\nRelevant project knowledge:");
        for entry in context {
            prompt.push_str(&format!("\n- {}", entry.content()));
        }
    }

    if !mcp_servers.is_empty() {
        prompt.push_str("\n\nAvailable MCP tool servers:");
        for server in mcp_servers {
            let endpoint = server.url().or_else(|| server.command()).unwrap_or("");
            prompt.push_str(&format!(
                "\n- {} ({}{})",
                server.name(),
                transport_label(server),
                if endpoint.is_empty() {
                    String::new()
                } else {
                    format!(": {endpoint}")
                }
            ));
        }
    }

    prompt
}

fn transport_label(server: &McpServer) -> &'static str {
    match server.transport() {
        codel00p_protocol::McpTransport::Stdio => "stdio",
        codel00p_protocol::McpTransport::Http => "http",
    }
}

fn plan_json(plan: &RunPlan, task: &str) -> Value {
    json!({
        "agent": {
            "id": plan.agent.id(),
            "name": plan.agent.name(),
            "provider": plan.agent.provider(),
            "model": plan.agent.model(),
        },
        "mcp_servers": plan.mcp_servers.iter().map(|server| json!({
            "id": server.id(),
            "name": server.name(),
            "transport": transport_label(server),
            "enabled": server.enabled(),
        })).collect::<Vec<_>>(),
        "context": plan.context.iter().map(|entry| json!({
            "id": entry.id(),
            "content": entry.content(),
        })).collect::<Vec<_>>(),
        "system_prompt": plan.system_prompt,
        "task": task,
    })
}

fn cloud_run(args: &[String]) -> CliResult<String> {
    let (connection, rest) = parse_connection(args)?;

    let mut agent_id = None;
    let mut task = String::new();
    let mut limit = 8usize;
    let mut plan_only = false;
    let mut json_output = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--task" => {
                task = required_value(&rest, index, "--task")?;
                index += 2;
            }
            "--limit" => {
                limit = required_value(&rest, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                index += 2;
            }
            "--plan" => {
                plan_only = true;
                index += 1;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag if !flag.starts_with("--") && agent_id.is_none() => {
                agent_id = Some(flag.to_string());
                index += 1;
            }
            flag => return Err(format!("unknown cloud run option: {flag}")),
        }
    }

    let agent_id = agent_id.ok_or_else(|| "cloud run expects an agent id".to_string())?;
    let project = connection.project()?.to_string();
    let client = connection.client()?;
    let plan = resolve_plan(&client, &project, &agent_id, &task, limit)?;

    if plan_only {
        if json_output {
            return serde_json::to_string(&plan_json(&plan, &task)).map_err(|e| e.to_string());
        }
        return Ok(render_plan(&plan, &task));
    }

    if task.trim().is_empty() {
        return Err("cloud run requires --task to execute (or pass --plan)".to_string());
    }

    // Execute a single turn against the agent's provider/model.
    let provider_client =
        build_provider_client_with(default_registry(), plan.agent.provider(), None)?;
    let request = InferenceRequest::builder(plan.agent.provider(), plan.agent.model())
        .message(ChatMessage::system(plan.system_prompt.clone()))
        .message(ChatMessage::user(task.clone()))
        .build();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let response = runtime
        .block_on(provider_client.complete(request))
        .map_err(|error| error.to_string())?;
    let answer = response.content.unwrap_or_default();

    if json_output {
        return serde_json::to_string(&json!({
            "agent": plan.agent.name(),
            "provider": plan.agent.provider(),
            "model": plan.agent.model(),
            "response": answer,
        }))
        .map_err(|e| e.to_string());
    }
    Ok(format!("{answer}\n"))
}

fn render_plan(plan: &RunPlan, task: &str) -> String {
    let mut output = format!(
        "agent: {} ({}/{})\n",
        plan.agent.name(),
        plan.agent.provider(),
        plan.agent.model()
    );
    output.push_str(&format!("task: {task}\n"));
    output.push_str(&format!("mcp servers: {}\n", plan.mcp_servers.len()));
    for server in &plan.mcp_servers {
        output.push_str(&format!(
            "  - {} ({})\n",
            server.name(),
            transport_label(server)
        ));
    }
    output.push_str(&format!("context entries: {}\n", plan.context.len()));
    output.push_str("--- system prompt ---\n");
    output.push_str(&plan.system_prompt);
    output.push('\n');
    output
}

/// The resolved cloud connection: where to reach the service and how to auth.
struct Connection {
    api_url: String,
    token: String,
    project: Option<String>,
}

impl Connection {
    fn client(&self) -> CliResult<CloudClient> {
        CloudClient::new(&self.api_url, &self.token)
    }

    fn project(&self) -> CliResult<&str> {
        self.project
            .as_deref()
            .ok_or_else(|| "missing --project (or CODEL00P_CLOUD_PROJECT)".to_string())
    }
}

/// Pulls `--api-url`, `--token`, and `--project` out of `args` (falling back to
/// the matching env vars) and returns the rest of the arguments untouched.
fn parse_connection(args: &[String]) -> CliResult<(Connection, Vec<String>)> {
    let mut api_url = env_value("CODEL00P_API_URL");
    let mut token = env_value("CODEL00P_TOKEN");
    let mut project = env_value("CODEL00P_CLOUD_PROJECT");
    let mut rest = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--api-url" => {
                api_url = Some(required_value(args, index, "--api-url")?);
                index += 2;
            }
            "--token" => {
                token = Some(required_value(args, index, "--token")?);
                index += 2;
            }
            "--project" => {
                project = Some(required_value(args, index, "--project")?);
                index += 2;
            }
            other => {
                rest.push(other.to_string());
                index += 1;
            }
        }
    }

    // Fall back to credentials stored by `codel00p login`.
    let stored = crate::credentials::load();
    let api_url = api_url.or(stored.api_url).ok_or_else(|| {
        "missing cloud API URL — pass --api-url, set CODEL00P_API_URL, or `codel00p login --api-url <url>`"
            .to_string()
    })?;
    let token = token.or(stored.token).ok_or_else(|| {
        "not signed in — run `codel00p login` (or pass --token / CODEL00P_TOKEN)".to_string()
    })?;

    Ok((
        Connection {
            api_url,
            token,
            project,
        },
        rest,
    ))
}

fn env_value(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn cloud_status(args: &[String]) -> CliResult<String> {
    let (connection, rest) = parse_connection(args)?;
    let json_output = parse_only_json(&rest, "cloud status")?;

    let viewer = connection.client()?.viewer()?;
    if json_output {
        return serde_json::to_string(&viewer).map_err(|error| error.to_string());
    }

    let mut output = format!("user: {}\n", viewer.user_id());
    if let Some(email) = viewer.email() {
        output.push_str(&format!("email: {email}\n"));
    }
    match (viewer.org(), viewer.org_role()) {
        (Some(org), role) => {
            output.push_str(&format!("organization: {} ({})\n", org.name(), org.id()));
            if let Some(role) = role {
                output.push_str(&format!("role: {}\n", role_label(role)));
            }
        }
        _ => output.push_str("organization: (none active)\n"),
    }
    Ok(output)
}

fn cloud_push(config: CliConfig, args: &[String]) -> CliResult<String> {
    let (connection, rest) = parse_connection(args)?;

    let mut status = MemoryStatus::Approved;
    let mut limit = None;
    let mut dry_run = false;
    let mut json_output = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--status" => {
                status = parse_status(&required_value(&rest, index, "--status")?)?;
                index += 2;
            }
            "--limit" => {
                limit = Some(
                    required_value(&rest, index, "--limit")?
                        .parse::<usize>()
                        .map_err(|_| "invalid --limit".to_string())?,
                );
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown cloud push option: {flag}")),
        }
    }

    let project = connection.project()?.to_string();
    let mut filter = MemoryListFilter::new(config.project.clone()).with_status(status);
    if let Some(limit) = limit {
        filter = filter.with_limit(limit);
    }

    let store = open_memory_store(&config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let candidates: Vec<(String, NewMemoryCandidate)> = records
        .iter()
        .map(|record| {
            (
                record.entry().id().to_string(),
                candidate_from_entry(record.entry()),
            )
        })
        .collect();

    if dry_run {
        return report_push(json_output, &candidates, &[], true);
    }

    let client = connection.client()?;
    let mut pushed = Vec::new();
    for (local_id, candidate) in &candidates {
        let remote = client.push_candidate(&project, candidate)?;
        pushed.push((local_id.clone(), remote));
    }
    report_push(json_output, &candidates, &pushed, false)
}

fn cloud_pull(config: CliConfig, args: &[String]) -> CliResult<String> {
    let (connection, rest) = parse_connection(args)?;

    let mut actor = "cloud-sync".to_string();
    let mut json_output = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--actor" => {
                actor = required_value(&rest, index, "--actor")?;
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown cloud pull option: {flag}")),
        }
    }

    let project = connection.project()?.to_string();
    let client = connection.client()?;
    let remote = client.list_memory(&project, Some("approved"))?;

    let mut store = open_memory_store(&config)?;
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for entry in &remote {
        let local_id = format!("cloud-{}", entry.id());
        // Idempotent: skip anything already imported.
        if store.get(&local_id).is_ok() {
            skipped.push(local_id);
            continue;
        }

        let source = MemorySource::turn(deserialize_id("cloud-sync")?, deserialize_id(entry.id())?)
            .with_uri(format!("codel00p://cloud/memory/{}", entry.id()));
        let mut input = MemoryCandidateInput::new(
            &local_id,
            config.project.clone(),
            entry.kind(),
            entry.content(),
            source,
        )
        .with_sensitivity(entry.sensitivity());
        for tag in entry.tags() {
            input = input.with_tag(tag);
        }

        // A rejected import (e.g. a duplicate of existing local content) is
        // skipped rather than failing the whole pull.
        match store.create_candidate(input) {
            Ok(_) => {
                store
                    .review(&local_id, ReviewDecision::approve(&actor))
                    .map_err(|error| error.to_string())?;
                imported.push(local_id);
            }
            Err(_) => skipped.push(local_id),
        }
    }

    if json_output {
        return serde_json::to_string(&json!({
            "imported": imported,
            "skipped": skipped,
        }))
        .map_err(|error| error.to_string());
    }

    Ok(format!(
        "imported {} approved memories, skipped {}\n",
        imported.len(),
        skipped.len()
    ))
}

fn candidate_from_entry(entry: &MemoryEntry) -> NewMemoryCandidate {
    NewMemoryCandidate {
        kind: entry.kind(),
        content: entry.content().to_string(),
        tags: entry.tags().to_vec(),
        sensitivity: entry.sensitivity(),
        source_uri: entry
            .source()
            .and_then(|source| source.uri().map(str::to_string)),
    }
}

fn report_push(
    json_output: bool,
    candidates: &[(String, NewMemoryCandidate)],
    pushed: &[(String, MemoryEntry)],
    dry_run: bool,
) -> CliResult<String> {
    if json_output {
        let pushed_json: Vec<Value> = pushed
            .iter()
            .map(|(local_id, remote)| json!({ "local_id": local_id, "remote_id": remote.id() }))
            .collect();
        return serde_json::to_string(&json!({
            "dry_run": dry_run,
            "selected": candidates.len(),
            "pushed": pushed_json,
        }))
        .map_err(|error| error.to_string());
    }

    if dry_run {
        return Ok(format!(
            "dry run: {} memories would be pushed\n",
            candidates.len()
        ));
    }
    Ok(format!("pushed {} memories\n", pushed.len()))
}

fn parse_only_json(args: &[String], command: &str) -> CliResult<bool> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        [flag, ..] => Err(format!("unknown {command} option: {flag}")),
    }
}

fn deserialize_id<T: DeserializeOwned>(value: &str) -> CliResult<T> {
    serde_json::from_value(Value::String(value.to_string())).map_err(|error| error.to_string())
}

fn parse_status(value: &str) -> CliResult<MemoryStatus> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

fn role_label(role: codel00p_protocol::OrgRole) -> &'static str {
    match role {
        codel00p_protocol::OrgRole::Admin => "admin",
        codel00p_protocol::OrgRole::Member => "member",
    }
}

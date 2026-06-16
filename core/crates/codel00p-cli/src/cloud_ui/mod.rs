//! The full-screen `codel00p cloud` dialog.
//!
//! Bare `codel00p cloud` on a terminal opens this; the scriptable subcommands
//! (`status`, `push`, `pull`, `run`) remain for non-TTY use. The pure model and
//! rendering live in [`model`] and [`view`]; the terminal lifecycle and blocking
//! loop are shared via [`crate::dialog`]. Every network call is blocking and runs
//! here in the driver — before the loop, or on an action/open key — never inside
//! the render closure.

mod model;
#[cfg(test)]
mod tests;
mod view;

use codel00p_protocol::{Agent, McpServer, MemoryEntry, Project, Viewer};

use crate::cloud::{ResolvedConnection, pull_summary, push_summary, resolve_connection};
use crate::cloud_client::CloudClient;
use crate::config::{CliConfig, CliResult};
use model::{CloudModel, EntityRow, Flow, ProjectRow};

/// Runs the cloud dialog. When not signed in, shows a sign-in hint rather than
/// erroring. The terminal lifecycle and loop come from [`crate::dialog`]; this
/// driver performs every blocking cloud call.
pub(crate) fn run(config: CliConfig) -> CliResult<String> {
    let connection = match resolve_connection() {
        Ok(connection) => connection,
        Err(_) => return run_unauthenticated(),
    };

    let mut model = build_signed_in_model(&connection);

    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay => {}
            Flow::Quit => return Ok(false),
            Flow::OpenProject(id) => open_project(&connection.client, model, &id),
            Flow::Push => run_action(model, push_action(&connection, &config)),
            Flow::Pull => run_action(model, pull_action(&connection, &config)),
        }
        Ok(true)
    })?;
    Ok("Closed cloud.\n".to_string())
}

/// Shows the "not signed in" dialog (no terminal-blocking network calls).
fn run_unauthenticated() -> CliResult<String> {
    let mut model = CloudModel::unauthenticated("You are not signed in to codel00p cloud.".into());
    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Quit => Ok(false),
            _ => Ok(true),
        }
    })?;
    Ok("Closed cloud.\n".to_string())
}

/// Builds the signed-in model: fetch the viewer (for status) and the org's
/// projects. Network failures surface as a status line rather than aborting, so the
/// dialog still opens.
fn build_signed_in_model(connection: &ResolvedConnection) -> CloudModel {
    let mut errors = Vec::new();
    let viewer_lines = match connection.client.viewer() {
        Ok(viewer) => viewer_lines(&viewer),
        Err(error) => {
            errors.push(error);
            vec!["viewer: (unavailable)".to_string()]
        }
    };
    let projects = match connection.client.list_projects() {
        Ok(projects) => projects.iter().map(project_row).collect(),
        Err(error) => {
            errors.push(error);
            Vec::new()
        }
    };

    let mut model = CloudModel::signed_in(viewer_lines, projects);
    if let Some(error) = errors.first() {
        model.set_status(error.clone());
    }
    model
}

/// Fetches the selected project's agents, MCP servers, and memory, then opens the
/// detail screen. A failed fetch leaves that tab empty and notes it on the status
/// line rather than aborting the dialog.
fn open_project(client: &CloudClient, model: &mut CloudModel, project_id: &str) {
    let Some(project) = model
        .projects
        .selected_item()
        .filter(|row| row.id == project_id)
        .cloned()
    else {
        return;
    };

    let mut error = None;
    let agents = match client.list_agents(project_id) {
        Ok(agents) => agents.iter().map(agent_row).collect(),
        Err(message) => {
            error.get_or_insert(message);
            Vec::new()
        }
    };
    let mcp = match client.list_mcp_servers(project_id) {
        Ok(servers) => servers.iter().map(mcp_row).collect(),
        Err(message) => {
            error.get_or_insert(message);
            Vec::new()
        }
    };
    let memory = match client.list_memory(project_id, Some("approved")) {
        Ok(entries) => entries.iter().map(memory_row).collect(),
        Err(message) => {
            error.get_or_insert(message);
            Vec::new()
        }
    };

    model.show_detail(project, agents, mcp, memory);
    if let Some(message) = error {
        model.set_status(message);
    }
}

/// Applies an action result to the status line.
fn run_action(model: &mut CloudModel, result: CliResult<String>) {
    match result {
        Ok(message) => model.set_status(message),
        Err(error) => model.set_status(error),
    }
}

fn push_action(connection: &ResolvedConnection, config: &CliConfig) -> CliResult<String> {
    let project = active_project(connection)?;
    push_summary(&connection.client, &project, config)
}

fn pull_action(connection: &ResolvedConnection, config: &CliConfig) -> CliResult<String> {
    let project = active_project(connection)?;
    pull_summary(&connection.client, &project, config)
}

/// The active cloud project for push/pull: `CODEL00P_CLOUD_PROJECT` (or stored).
fn active_project(connection: &ResolvedConnection) -> CliResult<String> {
    connection.project.clone().ok_or_else(|| {
        "set CODEL00P_CLOUD_PROJECT to push or pull (or use `codel00p cloud push/pull --project`)"
            .to_string()
    })
}

fn viewer_lines(viewer: &Viewer) -> Vec<String> {
    let mut lines = vec![format!("user: {}", viewer.user_id())];
    if let Some(email) = viewer.email() {
        lines.push(format!("email: {email}"));
    }
    match viewer.org() {
        Some(org) => lines.push(format!("org: {} ({})", org.name(), org.id())),
        None => lines.push("org: (none active)".to_string()),
    }
    lines
}

fn project_row(project: &Project) -> ProjectRow {
    ProjectRow {
        id: project.id().to_string(),
        name: project.name().to_string(),
        detail: project.repository_url().map(str::to_string),
    }
}

fn agent_row(agent: &Agent) -> EntityRow {
    EntityRow {
        label: agent.name().to_string(),
        detail: Some(format!("{} · {}", agent.provider(), agent.model())),
    }
}

fn mcp_row(server: &McpServer) -> EntityRow {
    let transport = match server.transport() {
        codel00p_protocol::McpTransport::Stdio => "stdio",
        codel00p_protocol::McpTransport::Http => "http",
    };
    let state = if server.enabled() {
        "enabled"
    } else {
        "disabled"
    };
    let target = server.command().or(server.url()).unwrap_or("");
    EntityRow {
        label: server.name().to_string(),
        detail: Some(format!("{transport} · {state} · {target}")),
    }
}

fn memory_row(entry: &MemoryEntry) -> EntityRow {
    EntityRow {
        label: entry.content().to_string(),
        detail: Some(format!("{:?} · {:?}", entry.kind(), entry.status())),
    }
}

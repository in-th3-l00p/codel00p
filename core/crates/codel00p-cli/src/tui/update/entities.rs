//! The cloud entity browser overlay: tabbed pickers over projects, agents, MCP
//! servers, memory, users, and orgs. Selections either set the active
//! provider/model in place or emit `Effect::Cloud(...)` fetches and org/project
//! switches for the event loop. `with_entities` applies an async fetch result to
//! the open browser (dropping it if the user already closed the overlay).

use super::*;

/// Runs `f` against the entity browser if one is open; otherwise drops the update
/// (the user closed the overlay before the fetch returned).
pub(super) fn with_entities(app: &mut App, f: impl FnOnce(&mut EntityBrowser)) {
    if let Overlay::Entities(browser) = &mut app.overlay {
        f(browser);
    }
}

pub(super) fn handle_entities_key(
    app: &mut App,
    mut browser: EntityBrowser,
    key: KeyEvent,
) -> Vec<Effect> {
    match key.code {
        KeyCode::Tab => {
            browser.tab = browser.tab.next();
            app.overlay = Overlay::Entities(browser);
            return Vec::new();
        }
        KeyCode::BackTab => {
            browser.tab = browser.tab.prev();
            app.overlay = Overlay::Entities(browser);
            return Vec::new();
        }
        _ => {}
    }

    match browser.tab {
        EntityTab::Projects => match browser.projects.on_key(key) {
            PickerOutcome::Selected => {
                if let Some(project) = browser.projects.selected_item().cloned() {
                    let id = project.id().to_string();
                    browser.selected_project = Some(project);
                    browser.tab = EntityTab::Agents;
                    browser.status = Some("Loading project…".to_string());
                    app.overlay = Overlay::Entities(browser);
                    return vec![
                        Effect::Cloud(CloudFetch::Agents(id.clone())),
                        Effect::Cloud(CloudFetch::Mcp(id.clone())),
                        Effect::Cloud(CloudFetch::Memory(id)),
                    ];
                }
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
        },
        EntityTab::Agents => {
            match browser.agents.on_key(key) {
                PickerOutcome::Selected => {
                    if let Some(agent) = browser.agents.selected_item() {
                        app.options.provider = agent.provider().to_string();
                        app.options.model = agent.model().to_string();
                        let name = agent.name().to_string();
                        let provider = agent.provider().to_string();
                        let model = agent.model().to_string();
                        app.conversation.push_notice(format!(
                            "Agent “{name}” selected — provider {provider}, model {model} (applies next turn)."
                        ));
                    }
                    Vec::new() // close overlay
                }
                PickerOutcome::Cancelled => Vec::new(),
                PickerOutcome::Pending => {
                    app.overlay = Overlay::Entities(browser);
                    Vec::new()
                }
            }
        }
        EntityTab::Mcp => {
            if matches!(browser.mcp.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Memory => {
            if matches!(browser.memory.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Users => {
            if matches!(browser.users.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Org => match browser.orgs.on_key(key) {
            PickerOutcome::Selected => {
                if let Some(org) = browser.orgs.selected_item() {
                    let org_id = org.id().to_string();
                    if app
                        .cloud
                        .viewer
                        .as_ref()
                        .and_then(|viewer| viewer.org())
                        .map(|active| active.id() == org_id)
                        .unwrap_or(false)
                    {
                        app.conversation
                            .push_notice(format!("Already using organization {}.", org.name()));
                        return Vec::new();
                    }
                    app.conversation.push_notice(format!(
                        "Opening browser to switch to organization {}…",
                        org.name()
                    ));
                    return vec![Effect::SwitchOrg(org_id)];
                }
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
        },
    }
}

pub(super) fn open_entities(app: &mut App, tab: EntityTab) -> Vec<Effect> {
    if !app.cloud.configured {
        app.conversation.push_notice(
            "Cloud not configured — run `codel00p auth login` to browse org entities.",
        );
        return Vec::new();
    }
    app.overlay = Overlay::Entities(EntityBrowser::new(tab));
    vec![
        Effect::Cloud(CloudFetch::Viewer),
        Effect::Cloud(CloudFetch::Orgs),
        Effect::Cloud(CloudFetch::Projects),
        Effect::Cloud(CloudFetch::Users),
    ]
}

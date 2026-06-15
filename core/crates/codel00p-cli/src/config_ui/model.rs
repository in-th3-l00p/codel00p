//! Pure state and transitions for the `codel00p config` dialog.
//!
//! Every state change happens in [`ConfigModel::update`], which maps a key event
//! to a [`Flow`]; all IO (terminal setup, reading current settings, persistence)
//! lives in the parent module. This keeps the dialog logic testable by feeding
//! synthetic key events and asserting on the resulting state.

use std::path::{Path, PathBuf};

use codel00p_providers::{AuthType, default_registry};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::CliResult;
use crate::providers::{CredentialStore, DotenvCredentialStore};
use crate::settings;

/// The section the dialog opens on. `codel00p config` opens [`Section::Menu`];
/// `codel00p config providers` jumps straight to [`Section::Providers`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Menu,
    Providers,
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Menu,
    Providers,
    Tools,
    Permissions,
}

/// What the event loop should do after an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Save,
    Quit,
}

/// Focus within the Providers screen: the provider list, or one of its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProvFocus {
    List,
    Key,
    Model,
    BaseUrl,
}

/// A row in the provider picker, projected from a registry profile.
pub(crate) struct ProviderRow {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) env_var: Option<&'static str>,
    pub(crate) is_api_key: bool,
    pub(crate) has_key: bool,
    pub(crate) default_base_url: Option<&'static str>,
    pub(crate) default_model: Option<&'static str>,
}

pub(crate) const TOOL_SETS: [&str; 8] = [
    "read", "edit", "command", "git", "web", "delegate", "learn", "all",
];
pub(crate) const PERMISSION_MODES: [&str; 3] = ["allow", "ask", "deny"];
pub(crate) const MENU_ITEMS: [&str; 5] = [
    "Providers",
    "Tools",
    "Permissions",
    "Save & quit",
    "Discard & quit",
];

pub(crate) struct ConfigModel {
    pub(crate) screen: Screen,
    pub(crate) menu_cursor: usize,
    pub(crate) providers: Vec<ProviderRow>,

    // Draft values (the configuration being edited).
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) permission_mode: String,
    pub(crate) tool_sets: Vec<String>,
    /// A key to write to `~/.codel00p/.env` on save: `(env var, value)`.
    pub(crate) pending_key: Option<(String, String)>,

    // Providers screen.
    pub(crate) prov_cursor: usize,
    pub(crate) prov_focus: ProvFocus,
    pub(crate) selected_env_var: Option<&'static str>,
    pub(crate) key_input: String,
    pub(crate) model_input: String,
    pub(crate) base_url_input: String,

    // Tools / Permissions cursors.
    pub(crate) tools_cursor: usize,
    pub(crate) perms_cursor: usize,

    pub(crate) project_scope: bool,
    pub(crate) workspace_start: PathBuf,
    pub(crate) dirty: bool,
}

impl ConfigModel {
    /// Builds the model, seeding the draft from the currently effective settings
    /// so the dialog edits the existing configuration rather than a blank slate.
    pub(crate) fn new(workspace_start: &Path, section: Section) -> Self {
        let store = DotenvCredentialStore::new();
        let registry = default_registry();
        let mut providers: Vec<ProviderRow> = registry
            .profiles()
            .map(|profile| ProviderRow {
                id: profile.id,
                display_name: profile.display_name,
                env_var: profile.env_vars.first().copied(),
                is_api_key: matches!(profile.auth_type, AuthType::ApiKey),
                has_key: profile.env_vars.iter().any(|var| store.get(var).is_some()),
                default_base_url: profile.default_base_url,
                default_model: profile.default_aux_model,
            })
            .collect();
        providers.sort_by_key(|row| row.id);

        let merged = settings::load_layered(workspace_start)
            .ok()
            .map(|resolved| resolved.merged);
        let get = |key: &str| {
            merged
                .as_ref()
                .and_then(|m| settings::effective_value(m, key).ok().flatten())
                .filter(|value| !value.is_empty())
        };
        let permission_mode = get("agent.permission_mode").unwrap_or_else(|| "ask".to_string());
        let tool_sets = get("agent.tool_sets")
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let prov_cursor = get("agent.provider")
            .as_deref()
            .and_then(|current| providers.iter().position(|row| row.id == current))
            .unwrap_or(0);
        let perms_cursor = PERMISSION_MODES
            .iter()
            .position(|mode| *mode == permission_mode)
            .unwrap_or(1);

        ConfigModel {
            screen: match section {
                Section::Menu => Screen::Menu,
                Section::Providers => Screen::Providers,
            },
            menu_cursor: 0,
            providers,
            provider: get("agent.provider"),
            model: get("agent.model"),
            base_url: get("agent.base_url"),
            permission_mode,
            tool_sets,
            pending_key: None,
            prov_cursor,
            prov_focus: ProvFocus::List,
            selected_env_var: None,
            key_input: String::new(),
            model_input: String::new(),
            base_url_input: String::new(),
            tools_cursor: 0,
            perms_cursor,
            project_scope: false,
            workspace_start: workspace_start.to_path_buf(),
            dirty: false,
        }
    }

    /// Feeds one key event, mutating state and returning the desired [`Flow`].
    pub(crate) fn update(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Flow::Quit;
        }
        match self.screen {
            Screen::Menu => self.update_menu(key),
            Screen::Providers => self.update_providers(key),
            Screen::Tools => self.update_tools(key),
            Screen::Permissions => self.update_permissions(key),
        }
    }

    fn update_menu(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Up => self.menu_cursor = self.menu_cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.menu_cursor + 1 < MENU_ITEMS.len() {
                    self.menu_cursor += 1;
                }
            }
            KeyCode::Char('s') => return Flow::Save,
            KeyCode::Char('q') | KeyCode::Esc => return Flow::Quit,
            KeyCode::Enter => match self.menu_cursor {
                0 => {
                    self.screen = Screen::Providers;
                    self.prov_focus = ProvFocus::List;
                }
                1 => self.screen = Screen::Tools,
                2 => self.screen = Screen::Permissions,
                3 => return Flow::Save,
                4 => return Flow::Quit,
                _ => {}
            },
            _ => {}
        }
        Flow::Continue
    }

    fn update_providers(&mut self, key: KeyEvent) -> Flow {
        match self.prov_focus {
            ProvFocus::List => match key.code {
                KeyCode::Up => self.prov_cursor = self.prov_cursor.saturating_sub(1),
                KeyCode::Down => {
                    if self.prov_cursor + 1 < self.providers.len() {
                        self.prov_cursor += 1;
                    }
                }
                KeyCode::Enter => self.select_provider(),
                KeyCode::Tab => self.prov_focus = ProvFocus::Key,
                KeyCode::Esc => self.leave_providers(),
                _ => {}
            },
            focus => match key.code {
                KeyCode::Esc => self.leave_providers(),
                KeyCode::Enter => self.leave_providers(),
                KeyCode::Tab => self.prov_focus = next_focus(focus),
                KeyCode::BackTab => self.prov_focus = prev_focus(focus),
                KeyCode::Backspace => {
                    self.field_mut(focus).pop();
                    self.dirty = true;
                }
                KeyCode::Char(c) => {
                    self.field_mut(focus).push(c);
                    self.dirty = true;
                }
                _ => {}
            },
        }
        Flow::Continue
    }

    fn update_tools(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Up => self.tools_cursor = self.tools_cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.tools_cursor + 1 < TOOL_SETS.len() {
                    self.tools_cursor += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_tool_set(),
            KeyCode::Esc => self.screen = Screen::Menu,
            _ => {}
        }
        Flow::Continue
    }

    fn update_permissions(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Up => self.perms_cursor = self.perms_cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.perms_cursor + 1 < PERMISSION_MODES.len() {
                    self.perms_cursor += 1;
                }
            }
            KeyCode::Enter => {
                self.permission_mode = PERMISSION_MODES[self.perms_cursor].to_string();
                self.dirty = true;
                self.screen = Screen::Menu;
            }
            KeyCode::Esc => self.screen = Screen::Menu,
            _ => {}
        }
        Flow::Continue
    }

    /// Picks the provider under the cursor, pre-filling the editable fields from
    /// its defaults and moving focus to the key field.
    fn select_provider(&mut self) {
        let Some(row) = self.providers.get(self.prov_cursor) else {
            return;
        };
        self.provider = Some(row.id.to_string());
        self.selected_env_var = row.env_var.filter(|_| row.is_api_key);
        self.key_input.clear();
        self.model_input = row.default_model.unwrap_or("").to_string();
        self.base_url_input = row.default_base_url.unwrap_or("").to_string();
        self.prov_focus = if self.selected_env_var.is_some() {
            ProvFocus::Key
        } else {
            ProvFocus::Model
        };
        self.dirty = true;
    }

    /// Applies the edited provider fields into the draft and returns to the menu.
    fn leave_providers(&mut self) {
        if self.provider.is_some() {
            self.model = non_empty(&self.model_input);
            self.base_url = non_empty(&self.base_url_input);
            if let (Some(var), Some(value)) = (self.selected_env_var, non_empty(&self.key_input)) {
                self.pending_key = Some((var.to_string(), value));
            }
        }
        self.screen = Screen::Menu;
    }

    fn toggle_tool_set(&mut self) {
        let name = TOOL_SETS[self.tools_cursor].to_string();
        if let Some(index) = self.tool_sets.iter().position(|set| *set == name) {
            self.tool_sets.remove(index);
        } else {
            self.tool_sets.push(name);
        }
        self.dirty = true;
    }

    fn field_mut(&mut self, focus: ProvFocus) -> &mut String {
        match focus {
            ProvFocus::Key => &mut self.key_input,
            ProvFocus::Model => &mut self.model_input,
            ProvFocus::BaseUrl => &mut self.base_url_input,
            ProvFocus::List => &mut self.key_input,
        }
    }

    /// Persists the draft: provider/model/base_url/permission_mode/tool_sets to the
    /// chosen config file, and any entered API key to `~/.codel00p/.env`.
    pub(crate) fn persist(&self) -> CliResult<String> {
        let path = if self.project_scope {
            settings::project_config_path(&self.workspace_start)
        } else {
            settings::user_config_path()
        };

        if let Some(provider) = &self.provider {
            settings::set_value(&path, "agent.provider", provider)?;
        }
        if let Some(model) = self.model.as_deref().filter(|m| !m.is_empty()) {
            settings::set_value(&path, "agent.model", model)?;
        }
        if let Some(base_url) = self.base_url.as_deref().filter(|u| !u.is_empty()) {
            settings::set_value(&path, "agent.base_url", base_url)?;
        }
        settings::set_value(&path, "agent.permission_mode", &self.permission_mode)?;
        if !self.tool_sets.is_empty() {
            settings::set_value(&path, "agent.tool_sets", &self.tool_sets.join(","))?;
        }
        if let Some((var, value)) = &self.pending_key
            && !value.is_empty()
        {
            DotenvCredentialStore::new().set(var, value)?;
        }

        let mut summary = String::from("Configuration saved.\n");
        if let Some(provider) = &self.provider {
            summary.push_str(&format!("  provider:    {provider}\n"));
        }
        if let Some(model) = self.model.as_deref().filter(|m| !m.is_empty()) {
            summary.push_str(&format!("  model:       {model}\n"));
        }
        summary.push_str(&format!("  permissions: {}\n", self.permission_mode));
        if !self.tool_sets.is_empty() {
            summary.push_str(&format!("  tools:       {}\n", self.tool_sets.join(", ")));
        }
        summary.push_str(&format!("  saved to:    {}\n", path.display()));
        Ok(summary)
    }
}

fn next_focus(focus: ProvFocus) -> ProvFocus {
    match focus {
        ProvFocus::List => ProvFocus::Key,
        ProvFocus::Key => ProvFocus::Model,
        ProvFocus::Model => ProvFocus::BaseUrl,
        ProvFocus::BaseUrl => ProvFocus::List,
    }
}

fn prev_focus(focus: ProvFocus) -> ProvFocus {
    match focus {
        ProvFocus::List => ProvFocus::BaseUrl,
        ProvFocus::Key => ProvFocus::List,
        ProvFocus::Model => ProvFocus::Key,
        ProvFocus::BaseUrl => ProvFocus::Model,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

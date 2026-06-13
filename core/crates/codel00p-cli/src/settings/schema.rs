use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::paths::{default_memory_db, expand_tilde};

pub type SettingsResult<T> = Result<T, String>;

/// Current on-disk schema version. Bump alongside a migration step.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_version: Option<u32>,
    #[serde(skip_serializing_if = "WorkspaceSettings::is_empty")]
    pub workspace: WorkspaceSettings,
    #[serde(skip_serializing_if = "AgentSettings::is_empty")]
    pub agent: AgentSettings,
    #[serde(skip_serializing_if = "PluginSettings::is_empty")]
    pub plugins: PluginSettings,
    #[serde(skip_serializing_if = "DelegationSettings::is_empty")]
    pub delegation: DelegationSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_db: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_policy_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_sets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember_permissions: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSettings {
    /// Ids of catalog plugins enabled for agent runs, in precedence order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Vec<String>>,
}

impl PluginSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.enabled, other.enabled);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelegationSettings {
    /// Maximum number of child agents that may run concurrently in a batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_children: Option<u32>,
}

impl DelegationSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(
            &mut self.max_concurrent_children,
            other.max_concurrent_children,
        );
    }
}

impl WorkspaceSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.organization_id, other.organization_id);
        take(&mut self.project_id, other.project_id);
        take(&mut self.project_name, other.project_name);
        take(&mut self.memory_db, other.memory_db);
    }
}

impl AgentSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.provider, other.provider);
        take(&mut self.model, other.model);
        take(&mut self.base_url, other.base_url);
        take(
            &mut self.provider_policy_preset,
            other.provider_policy_preset,
        );
        take(&mut self.max_iterations, other.max_iterations);
        take(&mut self.permission_mode, other.permission_mode);
        take(&mut self.tool_sets, other.tool_sets);
        take(&mut self.stream, other.stream);
        take(&mut self.remember_permissions, other.remember_permissions);
    }
}

impl Settings {
    pub(crate) fn merge(&mut self, other: Settings) {
        take(&mut self.config_version, other.config_version);
        self.workspace.merge(other.workspace);
        self.agent.merge(other.agent);
        self.plugins.merge(other.plugins);
        self.delegation.merge(other.delegation);
    }
}

fn take<T>(slot: &mut Option<T>, value: Option<T>) {
    if value.is_some() {
        *slot = value;
    }
}

/// The merged settings plus the file paths they came from.
pub struct ResolvedSettings {
    pub merged: Settings,
    pub user_path: PathBuf,
    pub project_path: Option<PathBuf>,
}

impl ResolvedSettings {
    pub fn organization_id(&self) -> String {
        self.merged
            .workspace
            .organization_id
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn project_id(&self) -> String {
        self.merged
            .workspace
            .project_id
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn project_name(&self) -> String {
        self.merged
            .workspace
            .project_name
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn memory_db(&self) -> PathBuf {
        match &self.merged.workspace.memory_db {
            Some(path) => expand_tilde(path),
            None => default_memory_db(),
        }
    }

    pub fn agent(&self) -> &AgentSettings {
        &self.merged.agent
    }
}

use std::collections::BTreeMap;
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
    #[serde(skip_serializing_if = "MemorySettings::is_empty")]
    pub memory: MemorySettings,
    #[serde(skip_serializing_if = "TuiSettings::is_empty")]
    pub tui: TuiSettings,
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
    /// Default tool-choice control (`auto`/`required`/`none`/a tool name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    /// Default structured-output request (`text`/`json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// Provider/model fallback routes (`<provider>:<model>[@<base_url>]`) the
    /// agent tries, in order, when the primary route fails with a fallback-
    /// eligible error. Overridden by repeated `--fallback` flags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember_permissions: Option<bool>,
    /// Where the agent's commands and filesystem ops execute. `local` (the
    /// default) runs them in the bare workspace; `docker` runs each command in an
    /// ephemeral container with the workspace bind-mounted (configured via
    /// `[agent.docker]`); `ssh` runs both commands and filesystem ops on a remote
    /// host where the workspace lives (remote-resident, configured via
    /// `[agent.ssh]`). This is the selection seam from initiative #7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_backend: Option<String>,
    /// Settings for the Docker execution backend (used when
    /// `execution_backend = "docker"`).
    #[serde(skip_serializing_if = "DockerSettings::is_empty")]
    pub docker: DockerSettings,
    /// Settings for the SSH (remote-resident) execution backend (used when
    /// `execution_backend = "ssh"`).
    #[serde(skip_serializing_if = "SshSettings::is_empty")]
    pub ssh: SshSettings,
    /// Org policy: when true, unattended turns (messaging gateway, scheduled
    /// jobs) that can execute shell commands must run on an isolating execution
    /// backend (e.g. `docker`). Fail-closed — such a turn is refused on a
    /// non-isolating backend. Defaults to off (unset). Part of initiative #7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_isolation_for_unattended: Option<bool>,
    /// Agent behavior toggles (the customizability layer). The first entries are
    /// the self-awareness facets; more behavior knobs are added here over time.
    #[serde(skip_serializing_if = "BehaviorSettings::is_empty")]
    pub behavior: BehaviorSettings,
    /// The active profile to apply this run (`agent.profile`). Names a bundle of
    /// settings defined under `[agent.profiles.<name>]` or one of the built-in
    /// presets (`autonomous`/`careful`/`manual`). A `--profile` flag overrides it
    /// for a single run. The capstone of the customizability layer (#12).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Named bundles of agent settings, serialized as `[agent.profiles.<name>]`.
    /// Each profile overrides only the subset of fields it sets; selecting one
    /// (via `agent.profile` or `--profile`) folds its values in as the baseline
    /// the existing config/flag layering then refines. A user profile with the
    /// same name as a built-in preset shadows it. The first table-of-tables in
    /// the settings schema.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, ProfileSettings>,
}

/// A named bundle of agent settings — the `[agent.profiles.<name>]` table. Every
/// field is optional so a profile overrides only the subset it sets; the rest
/// fall through to the existing per-setting layering. Fields mirror the agent
/// scalars a run resolves plus the full set of `[agent.behavior]` toggles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSettings {
    /// Human-readable summary shown by `config profiles list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // --- agent scalars a profile can override ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_sets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_backend: Option<String>,
    // --- behavior toggles (mirror BehaviorSettings) ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_knowledge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_prompt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_plan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_verify: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_test: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint_and_fix: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_critique: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_hints: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replan_on_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_budget: Option<u32>,
}

impl ProfileSettings {
    /// Per-key overwrite merge (project layer wins per profile field).
    fn merge(&mut self, other: Self) {
        take(&mut self.description, other.description);
        take(&mut self.provider, other.provider);
        take(&mut self.model, other.model);
        take(&mut self.base_url, other.base_url);
        take(&mut self.max_iterations, other.max_iterations);
        take(&mut self.permission_mode, other.permission_mode);
        take(&mut self.tool_sets, other.tool_sets);
        take(&mut self.execution_backend, other.execution_backend);
        take(&mut self.self_knowledge, other.self_knowledge);
        take(&mut self.self_state, other.self_state);
        take(&mut self.base_prompt, other.base_prompt);
        take(&mut self.auto_plan, other.auto_plan);
        take(&mut self.self_verify, other.self_verify);
        take(&mut self.auto_test, other.auto_test);
        take(&mut self.lint_and_fix, other.lint_and_fix);
        take(&mut self.self_critique, other.self_critique);
        take(&mut self.verify_iterations, other.verify_iterations);
        take(&mut self.test_command, other.test_command);
        take(&mut self.error_hints, other.error_hints);
        take(&mut self.replan_on_failure, other.replan_on_failure);
        take(&mut self.failure_budget, other.failure_budget);
    }

    /// A one-line summary of the overrides this profile sets, for `profiles list`
    /// / `show`. Skips `description` (shown separately).
    pub fn overrides_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(value) = &self.provider {
            parts.push(format!("provider={value}"));
        }
        if let Some(value) = &self.model {
            parts.push(format!("model={value}"));
        }
        if let Some(value) = &self.base_url {
            parts.push(format!("base_url={value}"));
        }
        if let Some(value) = self.max_iterations {
            parts.push(format!("max_iterations={value}"));
        }
        if let Some(value) = &self.permission_mode {
            parts.push(format!("permission_mode={value}"));
        }
        if let Some(value) = &self.tool_sets {
            parts.push(format!("tool_sets=[{}]", value.join(",")));
        }
        if let Some(value) = &self.execution_backend {
            parts.push(format!("execution_backend={value}"));
        }
        for (label, value) in [
            ("self_knowledge", self.self_knowledge),
            ("self_state", self.self_state),
            ("base_prompt", self.base_prompt),
            ("auto_plan", self.auto_plan),
            ("self_verify", self.self_verify),
            ("auto_test", self.auto_test),
            ("lint_and_fix", self.lint_and_fix),
            ("self_critique", self.self_critique),
            ("error_hints", self.error_hints),
            ("replan_on_failure", self.replan_on_failure),
        ] {
            if let Some(value) = value {
                parts.push(format!("{label}={value}"));
            }
        }
        if let Some(value) = self.verify_iterations {
            parts.push(format!("verify_iterations={value}"));
        }
        if let Some(value) = &self.test_command {
            parts.push(format!("test_command={value:?}"));
        }
        if let Some(value) = self.failure_budget {
            parts.push(format!("failure_budget={value}"));
        }
        parts.join(", ")
    }
}

/// The built-in profile presets, defined in code and selectable by name. A user
/// `[agent.profiles.<same-name>]` shadows the preset of that name. Kept as a
/// small table so they are easy to read and extend.
///
/// - `autonomous` — everything on, higher verify budget, full tool set: trust
///   the agent to drive end-to-end.
/// - `careful` — verify on, `ask` permission mode, lint-and-fix on, lower
///   autonomy: a human stays in the loop on privileged actions.
/// - `manual` — most autonomous behaviors off (no auto-plan/self-verify/
///   self-critique), read+edit only: "do exactly what I say."
pub fn builtin_profiles() -> BTreeMap<String, ProfileSettings> {
    let mut map = BTreeMap::new();
    map.insert(
        "autonomous".to_string(),
        ProfileSettings {
            description: Some(
                "Full autonomy: every behavior on, full tool set, higher verify budget."
                    .to_string(),
            ),
            permission_mode: Some("allow".to_string()),
            tool_sets: Some(vec!["all".to_string()]),
            self_knowledge: Some(true),
            self_state: Some(true),
            base_prompt: Some(true),
            auto_plan: Some(true),
            self_verify: Some(true),
            auto_test: Some(true),
            lint_and_fix: Some(true),
            self_critique: Some(true),
            verify_iterations: Some(5),
            error_hints: Some(true),
            replan_on_failure: Some(true),
            ..ProfileSettings::default()
        },
    );
    map.insert(
        "careful".to_string(),
        ProfileSettings {
            description: Some(
                "Human in the loop: ask before privileged actions, verify + lint-and-fix on."
                    .to_string(),
            ),
            permission_mode: Some("ask".to_string()),
            self_verify: Some(true),
            auto_test: Some(true),
            lint_and_fix: Some(true),
            self_critique: Some(true),
            verify_iterations: Some(3),
            error_hints: Some(true),
            replan_on_failure: Some(true),
            ..ProfileSettings::default()
        },
    );
    map.insert(
        "manual".to_string(),
        ProfileSettings {
            description: Some("Do exactly what I say: autonomy off (no auto-plan/verify/critique), read+edit only.".to_string()),
            tool_sets: Some(vec!["read".to_string(), "edit".to_string()]),
            auto_plan: Some(false),
            self_verify: Some(false),
            self_critique: Some(false),
            replan_on_failure: Some(false),
            ..ProfileSettings::default()
        },
    );
    map
}

/// Toggles that shape how the agent reasons about itself and its run — the start
/// of the customizability layer. Both self-awareness facets default to **on**
/// (smarter by default); set them `false` to opt out. New behavior knobs are
/// added here as `Option<_>` fields so existing configs keep working.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorSettings {
    /// Inject the agent's identity/capabilities ("who am I") block each turn and
    /// expose it via `self_describe`. Unset (the default) means enabled; set to
    /// `false` to drop the identity/capabilities block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_knowledge: Option<bool>,
    /// Include the live run-state line (iteration, context usage, plan progress)
    /// in the self block. Unset (the default) means enabled; set to `false` to
    /// drop the run-state line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_state: Option<bool>,
    /// Inject the default base operating prompt ("how I work": understand first,
    /// plan, change carefully, verify before declaring done) each turn. Unset (the
    /// default) means enabled; set to `false` to inject no base block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_prompt: Option<bool>,
    /// Include the base prompt's planning guidance (lay out a plan up front and
    /// keep it updated). Unset (the default) means enabled; set to `false` to omit
    /// the planning line so a minimal/manual profile stays quieter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_plan: Option<bool>,
    /// Master switch for the verify-before-done phase: when the agent signals it
    /// is done after a mutating turn, run the project's checks and do not complete
    /// until they pass (bounded by `verify_iterations`). Unset (the default) means
    /// enabled; set to `false` to complete immediately as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_verify: Option<bool>,
    /// Run the project's `test` check during the verify-before-done phase. Unset
    /// (the default) means enabled; set to `false` to skip the test check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_test: Option<bool>,
    /// Also run the project's `lint` check during verification and feed failures
    /// back. Default OFF (lint can be noisy); set to `true` to opt in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint_and_fix: Option<bool>,
    /// Run the metacognition / self-critique reflection step before final
    /// completion. Unset (the default) means enabled; set to `false` to skip it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_critique: Option<bool>,
    /// Max verify→fix attempts before completing with a not-verified signal.
    /// Unset defaults to 3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_iterations: Option<u32>,
    /// Explicit command override passed to the verification `run_checks` call
    /// instead of detection (e.g. `"cargo test -p mycrate"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_command: Option<String>,
    /// Attach failure classification (`error_kind`) + an actionable `hint` to
    /// failed tool results so the model self-corrects deliberately. Unset (the
    /// default) means enabled; set to `false` for bare `{ "error": ... }`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_hints: Option<bool>,
    /// When the same operation fails `failure_budget` times in a row, inject a
    /// step-back/replan nudge so the agent stops looping on a broken call. Unset
    /// (the default) means enabled; set to `false` to never nudge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replan_on_failure: Option<bool>,
    /// Consecutive same-operation failures before the replan nudge fires. Unset
    /// defaults to 3; 0 disables the budget entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_budget: Option<u32>,
    /// Inject the live "Workspace state" block each turn: the current git branch +
    /// working-tree summary, the project's detected test/build/lint commands, and
    /// the files the agent edited this turn. Unset (the default) means enabled; set
    /// to `false` to inject no workspace-state block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_context: Option<bool>,
    /// Proactively recall project memory relevant to the current task: rank
    /// approved memory against the latest user message (BM25, offline) so the
    /// most relevant memories surface automatically each turn. Unset (the
    /// default) means enabled; set to `false` to ignore the task and retrieve
    /// memory by configured filters only (prior behavior).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proactive_memory: Option<bool>,
    /// Inject the active agent's `persona.md` as the first ("who I am") system
    /// block each turn, ahead of the self block, base prompt, and project
    /// instructions. Unset (the default) means enabled; set to `false` to never
    /// inject the persona block. Only applies when an agent with a non-empty
    /// `persona.md` is active — the default agent has no persona regardless.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<bool>,
    /// Inject the active agent's capped curated memory (NOTES.md + USER.md — the
    /// always-in-context notes layer) as a system block each turn, after the
    /// persona/self block and before the base prompt. Unset (the default) means
    /// enabled; set to `false` to never inject the curated-memory block. Only
    /// produces a block when the files are non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curated_memory: Option<bool>,
    /// Enable the per-agent **curator**: consolidation of near-duplicate learned
    /// memories (`memory curate`) and skills (the near-duplicate pass of `skills
    /// curate`). Default **OFF** (opt-in) — unlike the always-on self-awareness
    /// knobs, curation archives knowledge, so a human opts in. Detection is
    /// offline shingle similarity; every consolidation is propose-for-review and
    /// reversible (archive-not-delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curator: Option<bool>,
}

impl BehaviorSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.self_knowledge, other.self_knowledge);
        take(&mut self.self_state, other.self_state);
        take(&mut self.base_prompt, other.base_prompt);
        take(&mut self.auto_plan, other.auto_plan);
        take(&mut self.self_verify, other.self_verify);
        take(&mut self.auto_test, other.auto_test);
        take(&mut self.lint_and_fix, other.lint_and_fix);
        take(&mut self.self_critique, other.self_critique);
        take(&mut self.verify_iterations, other.verify_iterations);
        take(&mut self.test_command, other.test_command);
        take(&mut self.error_hints, other.error_hints);
        take(&mut self.replan_on_failure, other.replan_on_failure);
        take(&mut self.failure_budget, other.failure_budget);
        take(&mut self.workspace_context, other.workspace_context);
        take(&mut self.proactive_memory, other.proactive_memory);
        take(&mut self.persona, other.persona);
        take(&mut self.curated_memory, other.curated_memory);
        take(&mut self.curator, other.curator);
    }

    /// Whether the identity/capabilities block is injected. Defaults to on.
    pub fn self_knowledge_enabled(&self) -> bool {
        self.self_knowledge.unwrap_or(true)
    }

    /// Whether the live run-state line is included. Defaults to on.
    pub fn self_state_enabled(&self) -> bool {
        self.self_state.unwrap_or(true)
    }

    /// Whether the base operating prompt is injected. Defaults to on.
    pub fn base_prompt_enabled(&self) -> bool {
        self.base_prompt.unwrap_or(true)
    }

    /// Whether the base prompt's planning guidance is included. Defaults to on.
    pub fn auto_plan_enabled(&self) -> bool {
        self.auto_plan.unwrap_or(true)
    }

    /// Whether the verify-before-done phase runs. Defaults to on.
    pub fn self_verify_enabled(&self) -> bool {
        self.self_verify.unwrap_or(true)
    }

    /// Whether the `test` check runs during verification. Defaults to on.
    pub fn auto_test_enabled(&self) -> bool {
        self.auto_test.unwrap_or(true)
    }

    /// Whether the `lint` check also runs and feeds failures back. Defaults OFF.
    pub fn lint_and_fix_enabled(&self) -> bool {
        self.lint_and_fix.unwrap_or(false)
    }

    /// Whether the self-critique reflection step runs. Defaults to on.
    pub fn self_critique_enabled(&self) -> bool {
        self.self_critique.unwrap_or(true)
    }

    /// Max verify→fix attempts before completing with a not-verified signal.
    /// Defaults to 3.
    pub fn verify_iterations_value(&self) -> u32 {
        self.verify_iterations.unwrap_or(3)
    }

    /// Explicit `run_checks` command override for verification, if configured.
    pub fn test_command_value(&self) -> Option<String> {
        self.test_command.clone()
    }

    /// Whether failed tool results carry classification + hint. Defaults to on.
    pub fn error_hints_enabled(&self) -> bool {
        self.error_hints.unwrap_or(true)
    }

    /// Whether the step-back/replan nudge fires on the failure budget. Defaults
    /// to on.
    pub fn replan_on_failure_enabled(&self) -> bool {
        self.replan_on_failure.unwrap_or(true)
    }

    /// Consecutive same-operation failures before the replan nudge. Defaults to 3.
    pub fn failure_budget_value(&self) -> u32 {
        self.failure_budget.unwrap_or(3)
    }

    /// Whether the live "Workspace state" block is injected. Defaults to on.
    pub fn workspace_context_enabled(&self) -> bool {
        self.workspace_context.unwrap_or(true)
    }

    /// Whether proactive task-aware memory recall is enabled. Defaults to on.
    pub fn proactive_memory_enabled(&self) -> bool {
        self.proactive_memory.unwrap_or(true)
    }

    /// Whether the active agent's persona block is injected. Defaults to on (the
    /// block only appears when an agent with a non-empty `persona.md` is active).
    pub fn persona_enabled(&self) -> bool {
        self.persona.unwrap_or(true)
    }

    /// Whether the active agent's capped curated memory block (NOTES.md +
    /// USER.md) is injected. Defaults to on (the block only appears when the
    /// files are non-empty).
    pub fn curated_memory_enabled(&self) -> bool {
        self.curated_memory.unwrap_or(true)
    }

    /// Whether the per-agent curator (near-duplicate memory/skill consolidation)
    /// is enabled. Defaults **OFF** — it is opt-in because it archives knowledge.
    pub fn curator_enabled(&self) -> bool {
        self.curator.unwrap_or(false)
    }
}

/// Configuration for the Docker execution backend. All fields are optional; the
/// harness applies its own defaults (image `alpine`, mount `/workspace`,
/// network `none`, map host user on) for anything left unset.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerSettings {
    /// Image to run commands in (e.g. `alpine`, `rust:1`). Defaults to `alpine`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Absolute path inside the container where the workspace is mounted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_mount: Option<String>,
    /// `--memory` limit (e.g. `512m`, `2g`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// `--cpus` limit (e.g. `1.5`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    /// `--network` mode (e.g. `none`, `bridge`, `host`). Defaults to `none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// Run the container as the host uid:gid so workspace files stay host-owned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_host_user: Option<bool>,
    /// Reuse one long-lived container per session (commands run via `docker
    /// exec`) instead of a fresh `docker run` per command. Defaults to `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_container: Option<bool>,
}

impl DockerSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.image, other.image);
        take(&mut self.container_mount, other.container_mount);
        take(&mut self.memory, other.memory);
        take(&mut self.cpus, other.cpus);
        take(&mut self.network, other.network);
        take(&mut self.map_host_user, other.map_host_user);
        take(&mut self.reuse_container, other.reuse_container);
    }
}

/// Configuration for the SSH (remote-resident) execution backend. The workspace
/// lives on the remote host; both commands and filesystem ops run there over
/// ssh. `host` and `workspace` are required when `execution_backend = "ssh"`;
/// everything else is optional and may come from the user's `~/.ssh/config`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SshSettings {
    /// Remote host (IP, DNS name, or `~/.ssh/config` alias). Required for ssh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Remote login user. Optional (defers to ssh config / current user).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Remote port. Optional (defers to ssh config / 22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Identity (private key) file. Optional (defers to ssh config / agent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// Absolute path on the remote host where the workspace lives. Required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

impl SshSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.host, other.host);
        take(&mut self.user, other.user);
        take(&mut self.port, other.port);
        take(&mut self.identity_file, other.identity_file);
        take(&mut self.workspace, other.workspace);
    }
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

/// How project memory is ranked for relevance retrieval (proactive recall and
/// `memory retrieve`). The default is offline BM25 — fully local, no network.
/// An **external** ranker hands memory content to a relevance/embedding service,
/// so it is governance-gated: it is used only when `ranker = "external"`, an
/// `external_url` is set, **and** `allow_external_ranking = true`. Any of those
/// missing falls back to BM25, fail-closed (memory content never leaves the host
/// unless the operator has explicitly opted in on all three).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorySettings {
    /// `internal` (default, offline BM25) or `external` (a remote ranking
    /// service). `external` only takes effect alongside `external_url` and
    /// `allow_external_ranking = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<String>,
    /// Base URL of the external ranking service (required for `ranker =
    /// "external"`). The host POSTs the query and candidate memory content here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
    /// Governance gate. Must be `true` for an external ranker to be used at all —
    /// it acknowledges that ranking sends memory content off the host. Defaults
    /// to off; leaving it unset keeps retrieval fully local even if a URL is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_external_ranking: Option<bool>,
}

impl MemorySettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.ranker, other.ranker);
        take(&mut self.external_url, other.external_url);
        take(
            &mut self.allow_external_ranking,
            other.allow_external_ranking,
        );
    }

    /// Whether external ranking is fully authorized: opted into the external
    /// ranker, a URL configured, and the governance gate explicitly enabled.
    /// Fail-closed — any missing piece keeps retrieval on offline BM25.
    pub fn external_ranking_enabled(&self) -> bool {
        self.ranker.as_deref() == Some("external")
            && self
                .external_url
                .as_deref()
                .is_some_and(|url| !url.is_empty())
            && self.allow_external_ranking.unwrap_or(false)
    }
}

/// Preferences for the interactive terminal UI (the agent TUI). These never
/// affect agent behavior — only what the status bar and overlays display.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiSettings {
    /// Show advanced status-bar info (model name, real token usage, and the
    /// context window meter). Unset (the default) hides it for a minimal bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_advanced: Option<bool>,
    /// Check for a newer codel00p release in the background on TUI startup,
    /// prompting to update if one is found. Unset (the default) means enabled;
    /// set to `false` to disable the check entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_updates: Option<bool>,
}

impl TuiSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.show_advanced, other.show_advanced);
        take(&mut self.check_updates, other.check_updates);
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
        take(&mut self.tool_choice, other.tool_choice);
        take(&mut self.response_format, other.response_format);
        take(&mut self.fallbacks, other.fallbacks);
        take(&mut self.stream, other.stream);
        take(&mut self.remember_permissions, other.remember_permissions);
        take(&mut self.execution_backend, other.execution_backend);
        self.docker.merge(other.docker);
        self.ssh.merge(other.ssh);
        take(
            &mut self.require_isolation_for_unattended,
            other.require_isolation_for_unattended,
        );
        self.behavior.merge(other.behavior);
        take(&mut self.profile, other.profile);
        // Per-profile merge: a profile present in both layers is merged field by
        // field (project wins per field); a profile only in `other` is inserted.
        // Distinct profiles thus coexist across layers.
        for (name, profile) in other.profiles {
            self.profiles.entry(name).or_default().merge(profile);
        }
    }
}

impl AgentSettings {
    /// Resolve the named profile against this settings' user-defined profiles and
    /// the built-in presets. A user `[agent.profiles.<name>]` shadows a built-in
    /// of the same name. Returns the resolved bundle, or an error listing the
    /// available names when `name` matches nothing.
    pub fn resolve_profile(&self, name: &str) -> SettingsResult<ProfileSettings> {
        if let Some(profile) = self.profiles.get(name) {
            return Ok(profile.clone());
        }
        if let Some(profile) = builtin_profiles().get(name) {
            return Ok(profile.clone());
        }
        Err(format!(
            "unknown agent profile `{name}`. Available profiles: {}",
            self.available_profile_names().join(", ")
        ))
    }

    /// All selectable profile names — user-defined plus built-in presets, sorted
    /// and de-duplicated (a user profile shadows a preset of the same name).
    pub fn available_profile_names(&self) -> Vec<String> {
        let mut names: std::collections::BTreeSet<String> =
            builtin_profiles().into_keys().collect();
        names.extend(self.profiles.keys().cloned());
        names.into_iter().collect()
    }

    /// Fold a resolved profile's values into this `AgentSettings` as the baseline
    /// the existing config/flag layering then refines. Only fields the profile
    /// actually sets are applied, and only over fields not already set by config
    /// (so a project-config scalar still wins over the profile). This is the
    /// "profile < config" rung of the precedence ladder.
    pub fn apply_profile(&mut self, profile: &ProfileSettings) {
        fill(&mut self.provider, &profile.provider);
        fill(&mut self.model, &profile.model);
        fill(&mut self.base_url, &profile.base_url);
        fill_copy(&mut self.max_iterations, profile.max_iterations);
        fill(&mut self.permission_mode, &profile.permission_mode);
        fill(&mut self.tool_sets, &profile.tool_sets);
        fill(&mut self.execution_backend, &profile.execution_backend);
        let b = &mut self.behavior;
        fill_copy(&mut b.self_knowledge, profile.self_knowledge);
        fill_copy(&mut b.self_state, profile.self_state);
        fill_copy(&mut b.base_prompt, profile.base_prompt);
        fill_copy(&mut b.auto_plan, profile.auto_plan);
        fill_copy(&mut b.self_verify, profile.self_verify);
        fill_copy(&mut b.auto_test, profile.auto_test);
        fill_copy(&mut b.lint_and_fix, profile.lint_and_fix);
        fill_copy(&mut b.self_critique, profile.self_critique);
        fill_copy(&mut b.verify_iterations, profile.verify_iterations);
        fill(&mut b.test_command, &profile.test_command);
        fill_copy(&mut b.error_hints, profile.error_hints);
        fill_copy(&mut b.replan_on_failure, profile.replan_on_failure);
        fill_copy(&mut b.failure_budget, profile.failure_budget);
    }
}

/// Set `slot` to `value` only when `slot` is unset and `value` is present —
/// "profile fills a gap config left open". Clone form for non-Copy values.
fn fill<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
    if slot.is_none() && value.is_some() {
        *slot = value.clone();
    }
}

/// Copy form of [`fill`] for `Copy` values.
fn fill_copy<T: Copy>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() && value.is_some() {
        *slot = value;
    }
}

impl Settings {
    pub(crate) fn merge(&mut self, other: Settings) {
        take(&mut self.config_version, other.config_version);
        self.workspace.merge(other.workspace);
        self.agent.merge(other.agent);
        self.plugins.merge(other.plugins);
        self.delegation.merge(other.delegation);
        self.memory.merge(other.memory);
        self.tui.merge(other.tui);
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

    pub fn memory(&self) -> &MemorySettings {
        &self.merged.memory
    }
}

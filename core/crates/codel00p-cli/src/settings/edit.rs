use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use toml_edit::{DocumentMut, Item, Table, Value};

use super::schema::{CONFIG_VERSION, Settings, SettingsResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueKind {
    Str,
    Bool,
    U32,
    StrList,
}

/// Every settable dotted key and its value kind. The single source of truth for
/// validation, coercion, and help.
const KEY_SPECS: &[(&str, ValueKind)] = &[
    ("workspace.organization_id", ValueKind::Str),
    ("workspace.project_id", ValueKind::Str),
    ("workspace.project_name", ValueKind::Str),
    ("workspace.memory_db", ValueKind::Str),
    ("agent.provider", ValueKind::Str),
    ("agent.model", ValueKind::Str),
    ("agent.base_url", ValueKind::Str),
    ("agent.provider_policy_preset", ValueKind::Str),
    ("agent.max_iterations", ValueKind::U32),
    ("agent.permission_mode", ValueKind::Str),
    ("agent.tool_sets", ValueKind::StrList),
    ("agent.stream", ValueKind::Bool),
    ("agent.remember_permissions", ValueKind::Bool),
    ("agent.execution_backend", ValueKind::Str),
    ("agent.docker.image", ValueKind::Str),
    ("agent.docker.container_mount", ValueKind::Str),
    ("agent.docker.memory", ValueKind::Str),
    ("agent.docker.cpus", ValueKind::Str),
    ("agent.docker.network", ValueKind::Str),
    ("agent.docker.map_host_user", ValueKind::Bool),
    ("agent.docker.reuse_container", ValueKind::Bool),
    ("agent.ssh.host", ValueKind::Str),
    ("agent.ssh.user", ValueKind::Str),
    ("agent.ssh.port", ValueKind::U32),
    ("agent.ssh.identity_file", ValueKind::Str),
    ("agent.ssh.workspace", ValueKind::Str),
    ("agent.require_isolation_for_unattended", ValueKind::Bool),
    ("agent.behavior.self_knowledge", ValueKind::Bool),
    ("agent.behavior.self_state", ValueKind::Bool),
    ("agent.behavior.base_prompt", ValueKind::Bool),
    ("agent.behavior.auto_plan", ValueKind::Bool),
    ("agent.behavior.self_verify", ValueKind::Bool),
    ("agent.behavior.auto_test", ValueKind::Bool),
    ("agent.behavior.lint_and_fix", ValueKind::Bool),
    ("agent.behavior.self_critique", ValueKind::Bool),
    ("agent.behavior.verify_iterations", ValueKind::U32),
    ("agent.behavior.test_command", ValueKind::Str),
    ("agent.behavior.error_hints", ValueKind::Bool),
    ("agent.behavior.replan_on_failure", ValueKind::Bool),
    ("agent.behavior.failure_budget", ValueKind::U32),
    ("agent.behavior.workspace_context", ValueKind::Bool),
    ("agent.behavior.proactive_memory", ValueKind::Bool),
    ("agent.behavior.persona", ValueKind::Bool),
    ("agent.behavior.curated_memory", ValueKind::Bool),
    ("agent.profile", ValueKind::Str),
    ("plugins.enabled", ValueKind::StrList),
    ("delegation.max_concurrent_children", ValueKind::U32),
    ("tui.show_advanced", ValueKind::Bool),
    ("tui.check_updates", ValueKind::Bool),
];

pub fn known_keys() -> Vec<&'static str> {
    KEY_SPECS.iter().map(|(key, _)| *key).collect()
}

fn key_kind(key: &str) -> Option<ValueKind> {
    if let Some(kind) = KEY_SPECS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, kind)| *kind)
    {
        return Some(kind);
    }
    // Dynamic `agent.profiles.<name>.<field>` keys: the profile alias is
    // arbitrary, so the kind is determined by the trailing field name.
    profile_field_kind(key)
}

/// The value kind of an `agent.profiles.<name>.<field>` key, or `None` if `key`
/// is not a (well-formed) profile key. The `<name>` segment is dynamic; the
/// field set mirrors [`super::schema::ProfileSettings`].
fn profile_field_kind(key: &str) -> Option<ValueKind> {
    let rest = key.strip_prefix("agent.profiles.")?;
    // Expect exactly `<name>.<field>`; reject deeper/shallower paths.
    let (name, field) = rest.split_once('.')?;
    if name.is_empty() || field.contains('.') {
        return None;
    }
    match field {
        "description" | "provider" | "model" | "base_url" | "permission_mode"
        | "execution_backend" | "test_command" => Some(ValueKind::Str),
        "tool_sets" => Some(ValueKind::StrList),
        "max_iterations" | "verify_iterations" | "failure_budget" => Some(ValueKind::U32),
        "self_knowledge" | "self_state" | "base_prompt" | "auto_plan" | "self_verify"
        | "auto_test" | "lint_and_fix" | "self_critique" | "error_hints" | "replan_on_failure" => {
            Some(ValueKind::Bool)
        }
        _ => None,
    }
}

/// Read the effective value of an `agent.profiles.<name>.<field>` key from the
/// merged profiles map. Returns `None` if the profile or field is unset.
fn profile_effective_value(agent: &super::schema::AgentSettings, key: &str) -> Option<String> {
    let rest = key.strip_prefix("agent.profiles.")?;
    let (name, field) = rest.split_once('.')?;
    let profile = agent.profiles.get(name)?;
    match field {
        "description" => profile.description.clone(),
        "provider" => profile.provider.clone(),
        "model" => profile.model.clone(),
        "base_url" => profile.base_url.clone(),
        "permission_mode" => profile.permission_mode.clone(),
        "execution_backend" => profile.execution_backend.clone(),
        "test_command" => profile.test_command.clone(),
        "tool_sets" => profile.tool_sets.as_ref().map(|sets| sets.join(",")),
        "max_iterations" => profile.max_iterations.map(|value| value.to_string()),
        "verify_iterations" => profile.verify_iterations.map(|value| value.to_string()),
        "failure_budget" => profile.failure_budget.map(|value| value.to_string()),
        "self_knowledge" => profile.self_knowledge.map(|value| value.to_string()),
        "self_state" => profile.self_state.map(|value| value.to_string()),
        "base_prompt" => profile.base_prompt.map(|value| value.to_string()),
        "auto_plan" => profile.auto_plan.map(|value| value.to_string()),
        "self_verify" => profile.self_verify.map(|value| value.to_string()),
        "auto_test" => profile.auto_test.map(|value| value.to_string()),
        "lint_and_fix" => profile.lint_and_fix.map(|value| value.to_string()),
        "self_critique" => profile.self_critique.map(|value| value.to_string()),
        "error_hints" => profile.error_hints.map(|value| value.to_string()),
        "replan_on_failure" => profile.replan_on_failure.map(|value| value.to_string()),
        _ => None,
    }
}

fn unknown_key_error(key: &str) -> String {
    format!(
        "unknown config key: {key}\nvalid keys:\n  {}\n  \
         agent.profiles.<name>.<field>  (field: description, provider, model, base_url, \
         max_iterations, permission_mode, tool_sets, execution_backend, or any \
         [agent.behavior] toggle)",
        known_keys().join("\n  ")
    )
}

/// Read the effective value of a dotted key from merged settings.
pub fn effective_value(settings: &Settings, key: &str) -> SettingsResult<Option<String>> {
    if key_kind(key).is_none() {
        return Err(unknown_key_error(key));
    }
    let workspace = &settings.workspace;
    let agent = &settings.agent;
    let value = match key {
        "workspace.organization_id" => workspace.organization_id.clone(),
        "workspace.project_id" => workspace.project_id.clone(),
        "workspace.project_name" => workspace.project_name.clone(),
        "workspace.memory_db" => workspace.memory_db.clone(),
        "agent.provider" => agent.provider.clone(),
        "agent.model" => agent.model.clone(),
        "agent.base_url" => agent.base_url.clone(),
        "agent.provider_policy_preset" => agent.provider_policy_preset.clone(),
        "agent.max_iterations" => agent.max_iterations.map(|value| value.to_string()),
        "agent.permission_mode" => agent.permission_mode.clone(),
        "agent.tool_sets" => agent.tool_sets.as_ref().map(|sets| sets.join(",")),
        "agent.stream" => agent.stream.map(|value| value.to_string()),
        "agent.remember_permissions" => agent.remember_permissions.map(|value| value.to_string()),
        "agent.execution_backend" => agent.execution_backend.clone(),
        "agent.docker.image" => agent.docker.image.clone(),
        "agent.docker.container_mount" => agent.docker.container_mount.clone(),
        "agent.docker.memory" => agent.docker.memory.clone(),
        "agent.docker.cpus" => agent.docker.cpus.clone(),
        "agent.docker.network" => agent.docker.network.clone(),
        "agent.docker.map_host_user" => agent.docker.map_host_user.map(|value| value.to_string()),
        "agent.docker.reuse_container" => {
            agent.docker.reuse_container.map(|value| value.to_string())
        }
        "agent.ssh.host" => agent.ssh.host.clone(),
        "agent.ssh.user" => agent.ssh.user.clone(),
        "agent.ssh.port" => agent.ssh.port.map(|value| value.to_string()),
        "agent.ssh.identity_file" => agent.ssh.identity_file.clone(),
        "agent.ssh.workspace" => agent.ssh.workspace.clone(),
        "agent.require_isolation_for_unattended" => agent
            .require_isolation_for_unattended
            .map(|value| value.to_string()),
        "agent.behavior.self_knowledge" => {
            agent.behavior.self_knowledge.map(|value| value.to_string())
        }
        "agent.behavior.self_state" => agent.behavior.self_state.map(|value| value.to_string()),
        "agent.behavior.base_prompt" => agent.behavior.base_prompt.map(|value| value.to_string()),
        "agent.behavior.auto_plan" => agent.behavior.auto_plan.map(|value| value.to_string()),
        "agent.behavior.self_verify" => agent.behavior.self_verify.map(|value| value.to_string()),
        "agent.behavior.auto_test" => agent.behavior.auto_test.map(|value| value.to_string()),
        "agent.behavior.lint_and_fix" => agent.behavior.lint_and_fix.map(|value| value.to_string()),
        "agent.behavior.self_critique" => {
            agent.behavior.self_critique.map(|value| value.to_string())
        }
        "agent.behavior.verify_iterations" => agent
            .behavior
            .verify_iterations
            .map(|value| value.to_string()),
        "agent.behavior.test_command" => agent.behavior.test_command.clone(),
        "agent.behavior.error_hints" => agent.behavior.error_hints.map(|value| value.to_string()),
        "agent.behavior.replan_on_failure" => agent
            .behavior
            .replan_on_failure
            .map(|value| value.to_string()),
        "agent.behavior.failure_budget" => {
            agent.behavior.failure_budget.map(|value| value.to_string())
        }
        "agent.behavior.workspace_context" => agent
            .behavior
            .workspace_context
            .map(|value| value.to_string()),
        "agent.behavior.proactive_memory" => agent
            .behavior
            .proactive_memory
            .map(|value| value.to_string()),
        "agent.behavior.persona" => agent.behavior.persona.map(|value| value.to_string()),
        "agent.behavior.curated_memory" => {
            agent.behavior.curated_memory.map(|value| value.to_string())
        }
        "agent.profile" => agent.profile.clone(),
        key if key.starts_with("agent.profiles.") => profile_effective_value(agent, key),
        "plugins.enabled" => settings.plugins.enabled.as_ref().map(|sets| sets.join(",")),
        "delegation.max_concurrent_children" => settings
            .delegation
            .max_concurrent_children
            .map(|value| value.to_string()),
        "tui.show_advanced" => settings.tui.show_advanced.map(|value| value.to_string()),
        "tui.check_updates" => settings.tui.check_updates.map(|value| value.to_string()),
        _ => None,
    };
    Ok(value)
}

fn read_document(path: &Path) -> SettingsResult<DocumentMut> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn coerce(kind: ValueKind, raw: &str) -> SettingsResult<Value> {
    match kind {
        ValueKind::Str => Ok(Value::from(raw)),
        ValueKind::Bool => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Ok(Value::from(true)),
            "false" | "no" | "off" | "0" => Ok(Value::from(false)),
            _ => Err(format!("expected a boolean (true/false), got `{raw}`")),
        },
        ValueKind::U32 => raw
            .trim()
            .parse::<u32>()
            .map(|value| Value::from(value as i64))
            .map_err(|_| format!("expected a non-negative integer, got `{raw}`")),
        ValueKind::StrList => {
            let mut array = toml_edit::Array::new();
            for item in raw.split(',') {
                let item = item.trim();
                if !item.is_empty() {
                    array.push(item);
                }
            }
            Ok(Value::Array(array))
        }
    }
}

/// Set a dotted key in the config document at `path` (created if absent),
/// stamping `config_version`, backing up, and writing atomically.
pub fn set_value(path: &Path, key: &str, raw: &str) -> SettingsResult<()> {
    let kind = key_kind(key).ok_or_else(|| unknown_key_error(key))?;
    let (table_path, field) = key.rsplit_once('.').expect("validated keys contain a dot");
    let value = coerce(kind, raw)?;

    let mut doc = read_document(path)?;
    ensure_version(&mut doc);
    let table = nested_table(&mut doc, table_path)?;
    table.insert(field, Item::Value(value));

    write_document(path, &doc)
}

/// Remove a dotted key. Returns whether anything was removed.
pub fn unset_value(path: &Path, key: &str) -> SettingsResult<bool> {
    if key_kind(key).is_none() {
        return Err(unknown_key_error(key));
    }
    let (table_path, field) = key.rsplit_once('.').expect("validated keys contain a dot");

    let mut doc = read_document(path)?;
    let removed = remove_nested(&mut doc, table_path, field);

    if removed {
        ensure_version(&mut doc);
        write_document(path, &doc)?;
    }
    Ok(removed)
}

/// Remove `field` from the table at the dotted `table_path`, pruning any tables
/// that become empty along the way (so `agent.docker.image` unset can leave both
/// `[agent.docker]` and `[agent]` clean if nothing else remains).
fn remove_nested(doc: &mut DocumentMut, table_path: &str, field: &str) -> bool {
    let segments: Vec<&str> = table_path.split('.').collect();
    remove_in(doc.as_table_mut(), &segments, field)
}

fn remove_in(table: &mut Table, segments: &[&str], field: &str) -> bool {
    match segments.split_first() {
        None => table.remove(field).is_some(),
        Some((head, rest)) => {
            let Some(child) = table.get_mut(head).and_then(Item::as_table_mut) else {
                return false;
            };
            let removed = remove_in(child, rest, field);
            if child.is_empty() {
                table.remove(head);
            }
            removed
        }
    }
}

fn ensure_version(doc: &mut DocumentMut) {
    if doc.get("config_version").is_none() {
        doc["config_version"] = Item::Value(Value::from(CONFIG_VERSION as i64));
    }
}

/// Bring a config file up to the current schema version, writing it back if the
/// file changed. v1 has no migration steps yet beyond stamping the version.
pub fn migrate(path: &Path) -> SettingsResult<u32> {
    let mut doc = read_document(path)?;
    let before = doc.to_string();
    ensure_version(&mut doc);
    if doc.to_string() != before {
        write_document(path, &doc)?;
    }
    Ok(CONFIG_VERSION)
}

/// Resolve (creating as needed) the table at the dotted `table_path`, e.g.
/// `agent` or `agent.docker`. Errors if a segment exists but is not a table.
fn nested_table<'a>(doc: &'a mut DocumentMut, table_path: &str) -> SettingsResult<&'a mut Table> {
    let mut table = doc.as_table_mut();
    let mut walked = String::new();
    for segment in table_path.split('.') {
        if walked.is_empty() {
            walked.push_str(segment);
        } else {
            walked.push('.');
            walked.push_str(segment);
        }
        table = table
            .entry(segment)
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| format!("config key `{walked}` is not a table"))?;
    }
    Ok(table)
}

fn write_document(path: &Path, doc: &DocumentMut) -> SettingsResult<()> {
    write_file_atomic(path, &doc.to_string())
}

/// Write `contents` to `path` atomically, backing up any existing file.
pub fn write_file_atomic(path: &Path, contents: &str) -> SettingsResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    if path.exists() {
        let _ = fs::copy(path, backup_path(path));
    }
    let tmp = tmp_path(path);
    fs::write(&tmp, contents)
        .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
    fs::rename(&tmp, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bak.{stamp}"));
    path.with_file_name(name)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// A commented starter config written by `config init`.
pub fn starter_template() -> String {
    format!(
        "# codel00p configuration. Edit with `codel00p config` or by hand.\n\
         # Precedence: defaults < this file < ./.codel00p/config.toml < env < CLI flags.\n\
         config_version = {CONFIG_VERSION}\n\
         \n\
         [workspace]\n\
         # organization_id = \"default\"\n\
         # project_id = \"default\"\n\
         # project_name = \"default\"\n\
         # memory_db = \"~/.codel00p/memory.sqlite\"\n\
         \n\
         [agent]\n\
         # provider = \"openrouter\"\n\
         # model = \"openai/gpt-4o-mini\"\n\
         # stream = true\n\
         # permission_mode = \"ask\"   # allow | ask | deny\n\
         # tool_sets = [\"read\"]       # read | edit | command | git | all\n\
         # execution_backend = \"local\"  # local | docker | ssh\n\
         # require_isolation_for_unattended = false  # force docker/ssh for unattended shell\n\
         # profile = \"careful\"         # default profile to apply (preset or [agent.profiles.<name>]); --profile overrides\n\
         \n\
         # [agent.profiles.<name>]      # a named bundle of overrides; select with --profile <name> or agent.profile\n\
         # built-in presets: autonomous (everything on), careful (ask + verify + lint), manual (read+edit, autonomy off)\n\
         # a user profile with a preset's name shadows it. Run `codel00p config profiles list`.\n\
         # [agent.profiles.review]\n\
         # description = \"Read-only code review\"\n\
         # tool_sets = [\"read\"]\n\
         # permission_mode = \"deny\"\n\
         # self_verify = false\n\
         \n\
         # [agent.behavior]             # how the agent reasons about itself (default on)\n\
         # self_knowledge = true        # inject the identity/capabilities block + self_describe tool\n\
         # self_state = true            # include the live run-state line (iteration, context, plan)\n\
         # base_prompt = true           # inject the base operating prompt (understand, plan, verify-before-done)\n\
         # auto_plan = true             # include the base prompt's planning guidance\n\
         # self_verify = true           # run the project's checks before completing a mutating turn\n\
         # auto_test = true             # run the `test` check during verification\n\
         # lint_and_fix = false         # also run `lint` and feed failures back (opt-in; can be noisy)\n\
         # self_critique = true         # one reflection turn to catch unverified/over-claimed work\n\
         # verify_iterations = 3        # max verify->fix attempts before completing as unverified\n\
         # test_command = \"cargo test\"  # explicit verification command override (bypasses detection)\n\
         # error_hints = true           # attach error_kind + hint to failed tool results\n\
         # replan_on_failure = true     # nudge to step back/replan after repeated same-op failures\n\
         # failure_budget = 3           # consecutive same-op failures before the replan nudge (0 = off)\n\
         # workspace_context = true     # inject the live workspace-state block (git status, detected commands, recent edits)\n\
         # proactive_memory = true      # recall project memory relevant to the current task (BM25, offline)\n\
         # persona = true               # inject the active agent's persona.md as the first system block\n\
         # curated_memory = true        # inject the capped curated notes layer (NOTES.md + USER.md) each turn\n\
         \n\
         # [agent.docker]               # used when execution_backend = \"docker\"\n\
         # image = \"alpine\"            # container image commands run in\n\
         # container_mount = \"/workspace\"  # where the workspace is bind-mounted\n\
         # network = \"none\"            # none | bridge | host\n\
         # memory = \"512m\"             # optional --memory limit\n\
         # cpus = \"1.5\"                # optional --cpus limit\n\
         # map_host_user = true         # run as host uid:gid so files stay host-owned\n\
         # reuse_container = true        # one warm container per session (docker exec) vs run-per-command\n\
         \n\
         # [agent.ssh]                  # used when execution_backend = \"ssh\" (remote-resident)\n\
         # host = \"myhost\"             # required: hostname/IP or ~/.ssh/config alias\n\
         # workspace = \"/srv/codel00p\" # required: absolute remote path the workspace lives at\n\
         # user = \"deploy\"             # optional: defers to ~/.ssh/config\n\
         # port = 22                    # optional: defers to ~/.ssh/config\n\
         # identity_file = \"~/.ssh/id_ed25519\"  # optional: defers to ~/.ssh/config / agent\n\
         \n\
         # [tui]\n\
         # show_advanced = false        # show model, token usage, and context meter in the TUI status bar\n\
         # check_updates = true         # check for a newer codel00p release on startup and prompt to update\n"
    )
}

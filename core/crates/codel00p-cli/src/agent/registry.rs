//! Local agent registry — the foundation of multi-agent personas (initiative #13).
//!
//! An **agent is a directory** `<base>/agents/<name>/` used as its own
//! `CODEL00P_HOME`. Switching agents = pointing `CODEL00P_HOME` at that dir, so
//! config, memory DB, and sessions isolate automatically via the existing home
//! boundary — no other subsystem changes.
//!
//! The base home (`$CODEL00P_HOME` or `~/.codel00p`) is the **default agent**:
//! with no agents created and no active pointer, behavior is byte-identical to
//! today (the base home). The registry always targets the *base* home, captured
//! independently of any later home override, so management ops stay stable
//! regardless of which agent is active.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const AGENTS_DIR: &str = "agents";
const AGENT_META_FILE: &str = "agent.toml";
const ACTIVE_POINTER_FILE: &str = "active_agent";
const PERSONA_FILE: &str = "persona.md";
const CONFIG_FILE: &str = "config.toml";
const SKILLS_DIR: &str = "skills";

pub type RegistryResult<T> = Result<T, String>;

/// The ORIGINAL codel00p home, resolved directly from the environment so it is
/// stable even after [`main`](crate) overrides `CODEL00P_HOME` for the active
/// agent. Mirrors [`crate::settings::home_dir`] but is deliberately decoupled
/// from any later override: `$CODEL00P_HOME` if set, else `<os-home>/.codel00p`.
///
/// main.rs calls this BEFORE overriding `CODEL00P_HOME` for the active agent, so
/// the registry always targets the true base. All registry functions take the
/// resolved base explicitly, so they never depend on the (possibly overridden)
/// process env after that point.
pub fn base_home() -> PathBuf {
    if let Some(dir) = env::var_os("CODEL00P_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codel00p")
}

/// Persisted per-agent metadata (`<agent>/agent.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Creation time, epoch milliseconds.
    pub created_at: u64,
}

/// A discovered agent: its metadata plus the resolved home directory.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub description: Option<String>,
    pub created_at: u64,
    pub home: PathBuf,
}

/// Options for [`create_agent`].
#[derive(Debug, Default, Clone)]
pub struct CreateOptions {
    pub description: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    /// Clone config + persona (+ skills) from this existing agent, with FRESH
    /// memory and sessions (Hermes semantics).
    pub clone_from: Option<String>,
    /// Persona text to seed `persona.md`; falls back to a small default.
    pub persona: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Validate that `name` is a single safe path segment: no separators, no `..`,
/// no leading dot, non-empty, reasonable charset. Prevents path traversal out
/// of `<base>/agents/`.
pub fn validate_name(name: &str) -> RegistryResult<()> {
    if name.is_empty() {
        return Err("agent name must not be empty".to_string());
    }
    if name == "." || name == ".." {
        return Err(format!("invalid agent name: `{name}`"));
    }
    if name.starts_with('.') {
        return Err(format!("agent name must not start with a dot: `{name}`"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(format!(
            "agent name must be a single path segment (no `/`, `\\`): `{name}`"
        ));
    }
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
    {
        return Err(format!(
            "agent name may only contain letters, digits, `-`, `_`, `.`: `{name}`"
        ));
    }
    Ok(())
}

/// `<base>/agents`.
fn agents_root(base: &Path) -> PathBuf {
    base.join(AGENTS_DIR)
}

/// The home directory of agent `name` under `base`: `<base>/agents/<name>`.
/// (Name is assumed validated; callers validate at the boundary.)
pub fn agent_home(base: &Path, name: &str) -> PathBuf {
    agents_root(base).join(name)
}

/// Whether agent `name` exists under `base` (its `agent.toml` is present).
pub fn agent_exists(base: &Path, name: &str) -> bool {
    if validate_name(name).is_err() {
        return false;
    }
    agent_home(base, name).join(AGENT_META_FILE).is_file()
}

/// List all agents under `base`, sorted by name. The base/default agent is
/// implicit (not returned here) — callers render it separately as "default".
pub fn list_agents(base: &Path) -> Vec<AgentInfo> {
    let root = agents_root(base);
    let mut agents = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return agents;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if validate_name(name).is_err() {
            continue;
        }
        let meta_path = path.join(AGENT_META_FILE);
        let Ok(text) = fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(meta) = toml::from_str::<AgentMeta>(&text) else {
            continue;
        };
        agents.push(AgentInfo {
            name: meta.name,
            description: meta.description,
            created_at: meta.created_at,
            home: path,
        });
    }
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    agents
}

/// Read one agent's info, or `None` if it does not exist.
pub fn agent_info(base: &Path, name: &str) -> Option<AgentInfo> {
    if validate_name(name).is_err() {
        return None;
    }
    let home = agent_home(base, name);
    let text = fs::read_to_string(home.join(AGENT_META_FILE)).ok()?;
    let meta = toml::from_str::<AgentMeta>(&text).ok()?;
    Some(AgentInfo {
        name: meta.name,
        description: meta.description,
        created_at: meta.created_at,
        home,
    })
}

/// Create a new agent under `base`. Writes `agent.toml`, seeds a starter
/// `config.toml` (with `agent.provider`/`agent.model` if given), and a
/// `persona.md`. With `clone_from`, copies the source agent's config + persona
/// (+ skills dir) but never its memory/sessions (fresh memory — Hermes
/// semantics). Memory/session stores are created lazily by their own code.
pub fn create_agent(base: &Path, name: &str, opts: &CreateOptions) -> RegistryResult<AgentInfo> {
    validate_name(name)?;
    if let Some(source) = &opts.clone_from {
        validate_name(source)?;
        if !agent_exists(base, source) {
            return Err(format!("clone source agent not found: `{source}`"));
        }
    }
    let home = agent_home(base, name);
    if home.join(AGENT_META_FILE).is_file() {
        return Err(format!("agent already exists: `{name}`"));
    }
    fs::create_dir_all(&home).map_err(|e| format!("failed to create {}: {e}", home.display()))?;

    // Config: clone the source's, else seed a starter with optional provider/model.
    let config_path = home.join(CONFIG_FILE);
    if let Some(source) = &opts.clone_from {
        let src_config = agent_home(base, source).join(CONFIG_FILE);
        if src_config.is_file() {
            fs::copy(&src_config, &config_path)
                .map_err(|e| format!("failed to copy config: {e}"))?;
        }
    } else {
        let config = seed_config(opts.provider.as_deref(), opts.model.as_deref());
        fs::write(&config_path, config)
            .map_err(|e| format!("failed to write {}: {e}", config_path.display()))?;
    }

    // Persona: explicit text > cloned source persona > default template.
    let persona_path = home.join(PERSONA_FILE);
    if let Some(text) = &opts.persona {
        fs::write(&persona_path, text)
            .map_err(|e| format!("failed to write {}: {e}", persona_path.display()))?;
    } else if let Some(source) = &opts.clone_from {
        let src_persona = agent_home(base, source).join(PERSONA_FILE);
        if src_persona.is_file() {
            fs::copy(&src_persona, &persona_path)
                .map_err(|e| format!("failed to copy persona: {e}"))?;
        } else {
            fs::write(&persona_path, default_persona(name))
                .map_err(|e| format!("failed to write {}: {e}", persona_path.display()))?;
        }
    } else {
        fs::write(&persona_path, default_persona(name))
            .map_err(|e| format!("failed to write {}: {e}", persona_path.display()))?;
    }

    // Skills: clone the source's skills dir if present (procedural memory).
    if let Some(source) = &opts.clone_from {
        let src_skills = agent_home(base, source).join(SKILLS_DIR);
        if src_skills.is_dir() {
            copy_dir_all(&src_skills, &home.join(SKILLS_DIR))
                .map_err(|e| format!("failed to copy skills: {e}"))?;
        }
    }

    let meta = AgentMeta {
        name: name.to_string(),
        description: opts.description.clone(),
        created_at: now_ms(),
    };
    write_meta(&home, &meta)?;

    Ok(AgentInfo {
        name: meta.name,
        description: meta.description,
        created_at: meta.created_at,
        home,
    })
}

fn write_meta(home: &Path, meta: &AgentMeta) -> RegistryResult<()> {
    let text =
        toml::to_string_pretty(meta).map_err(|e| format!("failed to serialize meta: {e}"))?;
    let path = home.join(AGENT_META_FILE);
    fs::write(&path, text).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Sets (or clears) a registry agent's description, rewriting its `agent.toml`.
/// Refuses unknown agents (the base/default agent has no `agent.toml`). An
/// empty/whitespace value clears the description.
pub fn set_agent_description(
    base: &Path,
    name: &str,
    description: Option<&str>,
) -> RegistryResult<()> {
    validate_name(name)?;
    if !agent_exists(base, name) {
        return Err(format!("agent not found: `{name}`"));
    }
    let home = agent_home(base, name);
    let path = home.join(AGENT_META_FILE);
    let text =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let mut meta = toml::from_str::<AgentMeta>(&text)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    meta.description = description
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    write_meta(&home, &meta)
}

/// Delete agent `name` (its whole home dir). Refuses unknown agents. The base
/// home is never reachable here (agents live strictly under `<base>/agents/`).
pub fn delete_agent(base: &Path, name: &str) -> RegistryResult<()> {
    validate_name(name)?;
    if !agent_exists(base, name) {
        return Err(format!("agent not found: `{name}`"));
    }
    let home = agent_home(base, name);
    fs::remove_dir_all(&home).map_err(|e| format!("failed to remove {}: {e}", home.display()))?;
    // Clearing a deleted active agent falls back to the default.
    if active_agent(base).as_deref() == Some(name) {
        set_active_agent(base, None)?;
    }
    Ok(())
}

/// Rename agent `old` to `new` (moves the directory, rewrites `agent.toml`).
pub fn rename_agent(base: &Path, old: &str, new: &str) -> RegistryResult<()> {
    validate_name(old)?;
    validate_name(new)?;
    if !agent_exists(base, old) {
        return Err(format!("agent not found: `{old}`"));
    }
    if agent_exists(base, new) {
        return Err(format!("agent already exists: `{new}`"));
    }
    let old_home = agent_home(base, old);
    let new_home = agent_home(base, new);
    fs::rename(&old_home, &new_home).map_err(|e| format!("failed to rename agent: {e}"))?;

    // Rewrite the name inside agent.toml.
    if let Some(text) = fs::read_to_string(new_home.join(AGENT_META_FILE)).ok()
        && let Ok(mut meta) = toml::from_str::<AgentMeta>(&text)
    {
        meta.name = new.to_string();
        write_meta(&new_home, &meta)?;
    }

    if active_agent(base).as_deref() == Some(old) {
        set_active_agent(base, Some(new))?;
    }
    Ok(())
}

/// The sticky active agent name from `<base>/active_agent`, or `None` (=
/// default/base home). The pointer lives in the BASE home so it is stable
/// regardless of which agent is currently active.
pub fn active_agent(base: &Path) -> Option<String> {
    let text = fs::read_to_string(base.join(ACTIVE_POINTER_FILE)).ok()?;
    let name = text.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Set or clear the sticky active agent pointer. `None` removes the pointer
/// (back to the default/base home).
pub fn set_active_agent(base: &Path, name: Option<&str>) -> RegistryResult<()> {
    let path = base.join(ACTIVE_POINTER_FILE);
    match name {
        Some(name) => {
            validate_name(name)?;
            fs::create_dir_all(base)
                .map_err(|e| format!("failed to create {}: {e}", base.display()))?;
            fs::write(&path, name)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        }
        None => {
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

/// Resolve the active agent for this invocation. Precedence:
/// explicit `--agent` flag > sticky active pointer > `None` (default/base home).
pub fn resolve_active(base: &Path, cli_flag: Option<&str>) -> Option<String> {
    if let Some(flag) = cli_flag {
        let trimmed = flag.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    active_agent(base)
}

/// A minimal starter config for a new agent home. Provider/model are written
/// only when supplied; otherwise the agent inherits via the layered config.
fn seed_config(provider: Option<&str>, model: Option<&str>) -> String {
    let mut out = String::from(
        "# codel00p agent configuration (this directory is the agent's CODEL00P_HOME).\n\
         # Layered: defaults < this file < ./.codel00p/config.toml < env < CLI flags.\n\n[agent]\n",
    );
    match provider {
        Some(p) => out.push_str(&format!("provider = \"{p}\"\n")),
        None => out.push_str("# provider = \"openrouter\"\n"),
    }
    match model {
        Some(m) => out.push_str(&format!("model = \"{m}\"\n")),
        None => out.push_str("# model = \"openai/gpt-4o-mini\"\n"),
    }
    out
}

/// A small default persona template for a fresh agent.
fn default_persona(name: &str) -> String {
    format!(
        "# Persona: {name}\n\n\
         You are **{name}**, a codel00p agent. Describe your durable identity,\n\
         expertise, voice, and operating preferences here. This file is injected\n\
         as your first system block each turn.\n"
    )
}

/// Recursively copy `src` into `dst` (used to clone an agent's skills dir).
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn create_then_list_and_home_path() {
        let dir = base();
        let base = dir.path();
        let info = create_agent(base, "alice", &CreateOptions::default()).expect("create");
        assert_eq!(info.name, "alice");
        assert_eq!(info.home, base.join("agents").join("alice"));
        assert_eq!(agent_home(base, "alice"), base.join("agents").join("alice"));
        assert!(agent_exists(base, "alice"));

        let listed = list_agents(base);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "alice");
        // persona + config seeded.
        assert!(info.home.join("persona.md").is_file());
        assert!(info.home.join("config.toml").is_file());
    }

    #[test]
    fn agent_toml_round_trips() {
        let dir = base();
        let base = dir.path();
        let opts = CreateOptions {
            description: Some("a helper".to_string()),
            ..Default::default()
        };
        create_agent(base, "bob", &opts).expect("create");
        let info = agent_info(base, "bob").expect("info");
        assert_eq!(info.description.as_deref(), Some("a helper"));
        assert!(info.created_at > 0);
    }

    #[test]
    fn create_with_provider_model_seeds_config() {
        let dir = base();
        let base = dir.path();
        let opts = CreateOptions {
            provider: Some("openrouter".to_string()),
            model: Some("x/y".to_string()),
            ..Default::default()
        };
        let info = create_agent(base, "carol", &opts).expect("create");
        let config = fs::read_to_string(info.home.join("config.toml")).expect("config");
        assert!(config.contains("provider = \"openrouter\""));
        assert!(config.contains("model = \"x/y\""));
    }

    #[test]
    fn clone_copies_config_and_persona_but_fresh_memory() {
        let dir = base();
        let base = dir.path();
        let src = create_agent(
            base,
            "src",
            &CreateOptions {
                persona: Some("# Persona: src\ncustom voice\n".to_string()),
                provider: Some("openrouter".to_string()),
                ..Default::default()
            },
        )
        .expect("create src");
        // Simulate the source having a memory db + a skill.
        fs::write(src.home.join("memory.sqlite"), b"not-empty").expect("memory");
        fs::create_dir_all(src.home.join("skills").join("greet")).expect("skill dir");
        fs::write(src.home.join("skills").join("greet").join("SKILL.md"), "hi").expect("skill");

        let cloned = create_agent(
            base,
            "clone",
            &CreateOptions {
                clone_from: Some("src".to_string()),
                ..Default::default()
            },
        )
        .expect("clone");

        // Config + persona + skills copied.
        assert_eq!(
            fs::read_to_string(cloned.home.join("persona.md")).unwrap(),
            "# Persona: src\ncustom voice\n"
        );
        assert!(
            fs::read_to_string(cloned.home.join("config.toml"))
                .unwrap()
                .contains("openrouter")
        );
        assert!(
            cloned
                .home
                .join("skills")
                .join("greet")
                .join("SKILL.md")
                .is_file()
        );
        // Memory is NOT copied (fresh).
        assert!(!cloned.home.join("memory.sqlite").exists());
    }

    #[test]
    fn delete_and_rename() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "del", &CreateOptions::default()).expect("create");
        delete_agent(base, "del").expect("delete");
        assert!(!agent_exists(base, "del"));

        create_agent(base, "old", &CreateOptions::default()).expect("create");
        rename_agent(base, "old", "new").expect("rename");
        assert!(!agent_exists(base, "old"));
        assert!(agent_exists(base, "new"));
        assert_eq!(agent_info(base, "new").unwrap().name, "new");
    }

    #[test]
    fn active_pointer_set_get_clear() {
        let dir = base();
        let base = dir.path();
        assert_eq!(active_agent(base), None);
        set_active_agent(base, Some("alice")).expect("set");
        assert_eq!(active_agent(base).as_deref(), Some("alice"));
        set_active_agent(base, None).expect("clear");
        assert_eq!(active_agent(base), None);
    }

    #[test]
    fn delete_active_clears_pointer() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "a", &CreateOptions::default()).expect("create");
        set_active_agent(base, Some("a")).expect("set");
        delete_agent(base, "a").expect("delete");
        assert_eq!(active_agent(base), None);
    }

    #[test]
    fn rename_active_follows() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "a", &CreateOptions::default()).expect("create");
        set_active_agent(base, Some("a")).expect("set");
        rename_agent(base, "a", "b").expect("rename");
        assert_eq!(active_agent(base).as_deref(), Some("b"));
    }

    #[test]
    fn resolve_active_precedence() {
        let dir = base();
        let base = dir.path();
        // none.
        assert_eq!(resolve_active(base, None), None);
        // pointer only.
        set_active_agent(base, Some("sticky")).expect("set");
        assert_eq!(resolve_active(base, None).as_deref(), Some("sticky"));
        // flag wins over pointer.
        assert_eq!(resolve_active(base, Some("flag")).as_deref(), Some("flag"));
        // empty flag falls back to pointer.
        assert_eq!(resolve_active(base, Some("  ")).as_deref(), Some("sticky"));
    }

    #[test]
    fn name_validation_rejects_traversal() {
        assert!(validate_name("../x").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a\\b").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("ok-name_1.2").is_ok());
    }

    #[test]
    fn create_rejects_bad_name_and_missing_clone_source() {
        let dir = base();
        let base = dir.path();
        assert!(create_agent(base, "../escape", &CreateOptions::default()).is_err());
        assert!(
            create_agent(
                base,
                "x",
                &CreateOptions {
                    clone_from: Some("nope".to_string()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

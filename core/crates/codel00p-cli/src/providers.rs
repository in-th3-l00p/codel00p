use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use codel00p_providers::{
    InferenceClient, ModelCatalogRequest, ProviderPolicy, ProviderRegistry, default_registry,
};

use crate::{config::CliResult, settings};

/// A model surfaced to the TUI model picker: the provider that owns it, its id, and
/// an optional human label. The provider is carried so selecting a row can switch
/// both provider and model in one step.
pub(crate) struct CatalogModel {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) note: Option<String>,
}

/// Lists a provider's models live via its catalog endpoint, for the TUI model
/// picker. Builds a default-registry client (allow-all policy, credential from env)
/// and normalizes each [`codel00p_providers::ProviderModel`] into a [`CatalogModel`].
/// Errors (no credential, network, unsupported provider) propagate so the caller can
/// fall back to the static catalog.
pub(crate) async fn list_provider_models(provider: &str) -> CliResult<Vec<CatalogModel>> {
    let client = build_provider_client_with(default_registry(), provider, None)?;
    let request = ModelCatalogRequest::builder(provider).build();
    let models = client
        .list_models(request)
        .await
        .map_err(|error| error.to_string())?;
    Ok(models
        .into_iter()
        .map(|model| CatalogModel {
            provider: provider.to_string(),
            note: model.display_name.filter(|name| name != &model.id),
            model: model.id,
        })
        .collect())
}

/// Build an inference client against a caller-supplied provider registry, so a
/// plugin-extended provider set (see [`crate::plugins`]) can route inference the
/// same way as built-in providers. Pass [`default_registry`] for the built-ins.
pub fn build_provider_client_with(
    registry: ProviderRegistry,
    provider: &str,
    policy_preset: Option<&str>,
) -> CliResult<InferenceClient> {
    if registry.resolve(provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }

    let policy = policy_preset
        .map(resolve_policy_preset)
        .transpose()?
        .unwrap_or_else(ProviderPolicy::allow_all);

    if registry.credential_from_env(provider).is_none() {
        let env_vars = registry
            .resolve(provider)
            .map(|profile| profile.env_vars.to_vec())
            .unwrap_or_default();
        return if env_vars.is_empty() {
            Err(format!("missing credential for provider `{provider}`"))
        } else {
            Err(format!(
                "missing credential for provider `{provider}`; set one of: {}",
                env_vars.join(", ")
            ))
        };
    }

    Ok(InferenceClient::builder()
        .registry(registry)
        .policy(policy)
        .credentials_from_env()
        .build())
}

fn resolve_policy_preset(id: &str) -> CliResult<ProviderPolicy> {
    ProviderPolicy::from_preset(id).ok_or_else(|| {
        let available = ProviderPolicy::presets()
            .iter()
            .map(|preset| preset.id)
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown provider policy preset: {id}; available presets: {available}")
    })
}

// --- `codel00p providers` command -----------------------------------------

/// Where provider API keys are stored. Backed by `~/.codel00p/.env` today; the
/// trait leaves room for an OS-keychain backend later without touching callers.
pub trait CredentialStore {
    fn get(&self, var: &str) -> Option<String>;
    fn set(&self, var: &str, value: &str) -> CliResult<()>;
    fn remove(&self, var: &str) -> CliResult<bool>;
}

pub struct DotenvCredentialStore {
    path: PathBuf,
}

impl DotenvCredentialStore {
    pub fn new() -> Self {
        Self {
            path: settings::env_file_path(),
        }
    }

    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.path)
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    fn entry_var(line: &str) -> Option<&str> {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return None;
        }
        trimmed.split_once('=').map(|(key, _)| key.trim())
    }
}

impl Default for DotenvCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for DotenvCredentialStore {
    fn get(&self, var: &str) -> Option<String> {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            return Some(value);
        }
        self.lines().into_iter().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            (key.trim() == var).then(|| value.trim().trim_matches('"').to_string())
        })
    }

    fn set(&self, var: &str, value: &str) -> CliResult<()> {
        let mut lines = self.lines();
        let entry = format!("{var}={value}");
        let mut replaced = false;
        for line in lines.iter_mut() {
            if Self::entry_var(line) == Some(var) {
                *line = entry.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            lines.push(entry);
        }
        let mut contents = lines.join("\n");
        contents.push('\n');
        settings::write_file_atomic(&self.path, &contents)?;
        restrict_permissions(&self.path);
        Ok(())
    }

    fn remove(&self, var: &str) -> CliResult<bool> {
        let mut removed = false;
        let kept: Vec<String> = self
            .lines()
            .into_iter()
            .filter(|line| {
                if Self::entry_var(line) == Some(var) {
                    removed = true;
                    false
                } else {
                    true
                }
            })
            .collect();
        if removed {
            let mut contents = kept.join("\n");
            if !contents.is_empty() {
                contents.push('\n');
            }
            settings::write_file_atomic(&self.path, &contents)?;
            restrict_permissions(&self.path);
        }
        Ok(removed)
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

pub fn run(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("list", &[][..]),
    };
    match command {
        "list" => providers_list(workspace_start),
        "use" => providers_use(workspace_start, rest),
        "set-key" => providers_set_key(rest),
        "remove-key" => providers_remove_key(rest),
        "show" => providers_show(rest),
        _ => Err(format!("unknown providers command: {command}")),
    }
}

fn providers_list(workspace_start: &Path) -> CliResult<String> {
    let resolved = settings::load_layered(workspace_start)?;
    let default_provider = resolved.agent().provider.clone();
    let store = DotenvCredentialStore::new();
    let registry = default_registry();

    let mut profiles: Vec<_> = registry.profiles().collect();
    profiles.sort_by_key(|profile| profile.id);

    let mut output = String::from("Providers ([x] = credential available):\n");
    for profile in profiles {
        let has_credential = profile.env_vars.iter().any(|var| store.get(var).is_some());
        let mark = if has_credential { "x" } else { " " };
        let default_tag = if default_provider.as_deref() == Some(profile.id) {
            "  (default)"
        } else {
            ""
        };
        output.push_str(&format!(
            "  [{mark}] {:<14} {:<20} stream={}{}\n",
            profile.id,
            format!("{:?}", profile.api_mode),
            profile.capabilities.streaming,
            default_tag,
        ));
    }
    output.push_str(
        "\nSet a key:  codel00p config providers set-key <id>\n\
         Use one:    codel00p config providers use <id> --model <model>\n\
         Details:    codel00p config providers show <id>\n",
    );
    Ok(output)
}

struct ProviderUseOptions {
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    preset: Option<String>,
    project: bool,
}

fn parse_provider_use(args: &[String]) -> CliResult<ProviderUseOptions> {
    let mut provider = None;
    let mut model = None;
    let mut base_url = None;
    let mut preset = None;
    let mut project = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--model" => {
                model = Some(value_after(args, index, "--model")?);
                index += 2;
            }
            "--base-url" => {
                base_url = Some(value_after(args, index, "--base-url")?);
                index += 2;
            }
            "--preset" => {
                preset = Some(value_after(args, index, "--preset")?);
                index += 2;
            }
            "--project" => {
                project = true;
                index += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown providers use option: {flag}"));
            }
            value => {
                if provider.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                provider = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(ProviderUseOptions {
        provider: provider
            .ok_or_else(|| "usage: providers use <id> [--model <model>]".to_string())?,
        model,
        base_url,
        preset,
        project,
    })
}

fn providers_use(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let options = parse_provider_use(args)?;
    let registry = default_registry();
    let profile = registry
        .resolve(&options.provider)
        .ok_or_else(|| format!("unknown provider: {}", options.provider))?;

    let path = if options.project {
        settings::project_config_path(workspace_start)
    } else {
        settings::user_config_path()
    };

    settings::set_value(&path, "agent.provider", profile.id)?;
    if let Some(model) = &options.model {
        settings::set_value(&path, "agent.model", model)?;
    }
    if let Some(base_url) = &options.base_url {
        settings::set_value(&path, "agent.base_url", base_url)?;
    }
    if let Some(preset) = &options.preset {
        settings::set_value(&path, "agent.provider_policy_preset", preset)?;
    }

    let mut output = format!("Default provider set to {}", profile.id);
    if let Some(model) = &options.model {
        output.push_str(&format!(" (model {model})"));
    }
    output.push_str(&format!(" in {}.\n", path.display()));

    let store = DotenvCredentialStore::new();
    if !profile.env_vars.iter().any(|var| store.get(var).is_some()) {
        output.push_str(&format!(
            "No credential found — run: codel00p config providers set-key {}\n",
            profile.id
        ));
    }
    Ok(output)
}

fn providers_set_key(args: &[String]) -> CliResult<String> {
    let provider = args
        .first()
        .ok_or_else(|| "usage: providers set-key <id> [<key>]".to_string())?;
    let registry = default_registry();
    let profile = registry
        .resolve(provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    let var = *profile
        .env_vars
        .first()
        .ok_or_else(|| format!("provider {} takes no API key", profile.id))?;

    let key = match args.get(1) {
        Some(key) => key.clone(),
        None => prompt_secret(&format!("Enter API key for {} ({var}): ", profile.id))?,
    };
    if key.trim().is_empty() {
        return Err("no key provided".to_string());
    }

    DotenvCredentialStore::new().set(var, key.trim())?;
    Ok(format!(
        "Stored {var} in {}.\n",
        settings::env_file_path().display()
    ))
}

fn providers_remove_key(args: &[String]) -> CliResult<String> {
    let provider = args
        .first()
        .ok_or_else(|| "usage: providers remove-key <id>".to_string())?;
    let registry = default_registry();
    let profile = registry
        .resolve(provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    let store = DotenvCredentialStore::new();
    let mut removed_any = false;
    for var in profile.env_vars {
        removed_any |= store.remove(var)?;
    }
    Ok(if removed_any {
        format!("Removed credential(s) for {}.\n", profile.id)
    } else {
        format!("No stored credential for {}.\n", profile.id)
    })
}

fn providers_show(args: &[String]) -> CliResult<String> {
    let provider = args
        .first()
        .ok_or_else(|| "usage: providers show <id>".to_string())?;
    let registry = default_registry();
    let profile = registry
        .resolve(provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    let store = DotenvCredentialStore::new();

    let mut output = format!("{} ({})\n", profile.id, profile.display_name);
    if !profile.aliases.is_empty() {
        output.push_str(&format!("  aliases:      {}\n", profile.aliases.join(", ")));
    }
    output.push_str(&format!("  api mode:     {:?}\n", profile.api_mode));
    output.push_str(&format!(
        "  base url:     {}\n",
        profile.default_base_url.unwrap_or("(set with --base-url)")
    ));
    output.push_str(&format!(
        "  streaming:    {}\n",
        profile.capabilities.streaming
    ));
    output.push_str(&format!(
        "  env vars:     {}\n",
        profile.env_vars.join(", ")
    ));
    let credential = profile
        .env_vars
        .iter()
        .find_map(|var| store.get(var).map(|_| *var));
    output.push_str(&match credential {
        Some(var) => format!("  credential:   set via {var}\n"),
        None => format!(
            "  credential:   missing — run: codel00p config providers set-key {}\n",
            profile.id
        ),
    });
    Ok(output)
}

/// Interactive provider setup used by `codel00p config setup`.
pub fn setup(workspace_start: &Path) -> CliResult<String> {
    let registry = default_registry();
    let mut profiles: Vec<_> = registry.profiles().collect();
    profiles.sort_by_key(|profile| profile.id);

    let mut stderr = io::stderr();
    writeln!(stderr, "codel00p setup — choose a provider:").ok();
    for (index, profile) in profiles.iter().enumerate() {
        writeln!(stderr, "  {}) {}", index + 1, profile.id).ok();
    }
    let choice = prompt_line(&mut stderr, "Provider number or id: ")?;
    let profile = choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|number| profiles.get(number.wrapping_sub(1)).copied())
        .or_else(|| registry.resolve(choice.trim()))
        .ok_or_else(|| format!("unknown provider: {}", choice.trim()))?;

    if let Some(var) = profile.env_vars.first() {
        let store = DotenvCredentialStore::new();
        if store.get(var).is_none() {
            let key = prompt_secret(&format!(
                "API key for {} ({var}, blank to skip): ",
                profile.id
            ))?;
            if !key.trim().is_empty() {
                store.set(var, key.trim())?;
            }
        }
    }

    let model = prompt_line(&mut stderr, "Default model (blank to skip): ")?;
    let path = settings::user_config_path();
    settings::set_value(&path, "agent.provider", profile.id)?;
    if !model.trim().is_empty() {
        settings::set_value(&path, "agent.model", model.trim())?;
    }

    let _ = workspace_start;
    Ok(format!(
        "Setup complete. Default provider is {}{}.\n",
        profile.id,
        if model.trim().is_empty() {
            String::new()
        } else {
            format!(", model {}", model.trim())
        }
    ))
}

fn value_after(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

fn prompt_line(stderr: &mut io::Stderr, prompt: &str) -> CliResult<String> {
    write!(stderr, "{prompt}").ok();
    stderr.flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

fn prompt_secret(prompt: &str) -> CliResult<String> {
    let mut stderr = io::stderr();
    prompt_line(&mut stderr, prompt)
}

#[cfg(test)]
mod tests;

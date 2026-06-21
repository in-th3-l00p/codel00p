//! CLI agent option types and flag parsing.

use super::*;

#[derive(Clone)]
pub(crate) struct AgentRunOptions {
    pub(crate) prompt: String,
    pub(crate) workspace: PathBuf,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) provider_policy_preset: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) max_iterations: Option<u32>,
    pub(crate) json_events: bool,
    pub(crate) stream_events: bool,
    pub(crate) stream: bool,
    pub(crate) tool_sets: Vec<AgentToolSet>,
    /// Optional control over whether/which tool the model must call. `None`
    /// leaves the provider default (auto) in place — the default CLI path.
    pub(crate) tool_choice: Option<codel00p_harness::ToolChoice>,
    /// Optional structured-output (JSON mode) request. `None` is plain text.
    pub(crate) response_format: Option<codel00p_harness::ResponseFormat>,
    pub(crate) permission_mode: CliPermissionMode,
    pub(crate) remember_permissions: bool,
    pub(crate) mcp_servers: Vec<McpServerSpec>,
    /// Provider/model routes the inference client tries, in order, when the
    /// primary route fails with a fallback-eligible error. Empty by default.
    pub(crate) fallback_routes: Vec<InferenceFallbackRoute>,
    /// When set, the turn is a messaging-gateway turn: privileged tools pause
    /// for a remote chat user's `/approve` decision instead of using the local
    /// CLI permission mode. See [`GatewayApprovalPolicy`].
    pub(crate) gateway_approval: Option<GatewayApproval>,
    /// True when the turn runs without an interactive operator at the keyboard
    /// (messaging gateway, scheduled/cron job). Used to enforce the
    /// `agent.require_isolation_for_unattended` org policy: such a turn may not
    /// run shell-capable tool sets on a non-isolating execution backend.
    pub(crate) unattended: bool,
    /// The resolved active profile name (`--profile` > `agent.profile`), if any.
    /// Surfaced to the agent's self-awareness so the self block reflects it.
    pub(crate) profile: Option<String>,
}

/// Routes a gateway turn's privileged-tool permissions through a remote chat
/// user's `/approve` / `/deny` decisions, backed by a file [`ApprovalStore`].
#[derive(Clone)]
pub(crate) struct GatewayApproval {
    pub(super) store: ApprovalStore,
    pub(super) conversation: String,
    /// A one-shot grant: the single tool the remote user just approved may run
    /// once without re-prompting. Any *other* privileged tool re-prompts.
    pub(super) granted_tool: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentToolSet {
    Read,
    Edit,
    Command,
    Git,
    Web,
    Delegate,
    Learn,
    Pipeline,
    Code,
    All,
}

impl AgentToolSet {
    /// The canonical lowercase label, mirroring [`parse_agent_tool_set`]. Used to
    /// describe the agent's capabilities to itself (self-awareness).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            AgentToolSet::Read => "read",
            AgentToolSet::Edit => "edit",
            AgentToolSet::Command => "command",
            AgentToolSet::Git => "git",
            AgentToolSet::Web => "web",
            AgentToolSet::Delegate => "delegate",
            AgentToolSet::Learn => "learn",
            AgentToolSet::Pipeline => "pipeline",
            AgentToolSet::Code => "code",
            AgentToolSet::All => "all",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliPermissionMode {
    Allow,
    Ask,
    Deny,
}

impl CliPermissionMode {
    /// The canonical lowercase label, mirroring [`parse_permission_mode`].
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CliPermissionMode::Allow => "allow",
            CliPermissionMode::Ask => "ask",
            CliPermissionMode::Deny => "deny",
        }
    }
}

pub(super) enum AgentSessionMode {
    Fresh,
    Resume,
}

pub(super) fn parse_agent_run_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    let Some(prompt) = args.first() else {
        return Err("missing agent prompt".to_string());
    };

    let mut options = parse_agent_flag_options(defaults, args, 1, "run")?;
    options.prompt = prompt.to_string();
    Ok(options)
}

pub(super) fn parse_agent_chat_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    parse_agent_flag_options(defaults, args, 0, "chat")
}

fn parse_agent_flag_options(
    defaults: &AgentSettings,
    args: &[String],
    start: usize,
    context: &str,
) -> CliResult<AgentRunOptions> {
    let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
    let mut provider = None;
    let mut model = None;
    let mut provider_policy_preset = None;
    let mut base_url = None;
    let mut session_id = None;
    let mut max_iterations = None;
    let mut json_events = false;
    let mut stream_events = false;
    let mut stream = None;
    let mut tool_sets = Vec::new();
    let mut tool_choice = None;
    let mut response_format = None;
    let mut permission_mode = None;
    let mut remember_permissions = None;
    let mut mcp_servers = Vec::new();
    let mut fallback_routes = Vec::new();
    let mut profile_flag = None;
    let mut index = start;

    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                profile_flag = Some(required_value(args, index, "--profile")?);
                index += 2;
            }
            "--workspace" => {
                workspace = PathBuf::from(required_value(args, index, "--workspace")?);
                index += 2;
            }
            "--provider" => {
                provider = Some(required_value(args, index, "--provider")?);
                index += 2;
            }
            "--model" => {
                model = Some(required_value(args, index, "--model")?);
                index += 2;
            }
            "--provider-policy-preset" => {
                provider_policy_preset =
                    Some(required_value(args, index, "--provider-policy-preset")?);
                index += 2;
            }
            "--base-url" => {
                base_url = Some(required_value(args, index, "--base-url")?);
                index += 2;
            }
            "--session-id" => {
                session_id = Some(required_value(args, index, "--session-id")?);
                index += 2;
            }
            "--max-iterations" => {
                let value = required_value(args, index, "--max-iterations")?
                    .parse::<u32>()
                    .map_err(|_| "invalid --max-iterations".to_string())?;
                max_iterations = Some(value);
                index += 2;
            }
            "--json-events" => {
                json_events = true;
                index += 1;
            }
            "--stream-events" => {
                stream_events = true;
                index += 1;
            }
            "--stream" => {
                stream = Some(true);
                index += 1;
            }
            "--tool-set" => {
                let value = required_value(args, index, "--tool-set")?;
                tool_sets.push(parse_agent_tool_set(&value)?);
                index += 2;
            }
            "--tool-choice" => {
                let value = required_value(args, index, "--tool-choice")?;
                tool_choice = Some(parse_tool_choice(&value)?);
                index += 2;
            }
            "--response-format" => {
                let value = required_value(args, index, "--response-format")?;
                response_format = Some(parse_response_format(&value)?);
                index += 2;
            }
            "--permission-mode" => {
                let value = required_value(args, index, "--permission-mode")?;
                permission_mode = Some(parse_permission_mode(&value)?);
                index += 2;
            }
            "--remember-permissions" => {
                remember_permissions = Some(true);
                index += 1;
            }
            "--mcp-server" => {
                let value = required_value(args, index, "--mcp-server")?;
                mcp_servers.push(parse_mcp_server(&value)?);
                index += 2;
            }
            "--fallback" => {
                let value = required_value(args, index, "--fallback")?;
                fallback_routes.push(parse_fallback_route(&value)?);
                index += 2;
            }
            flag => return Err(format!("unknown agent {context} option: {flag}")),
        }
    }

    // Resolution precedence (most specific wins): built-in/default < selected
    // profile's values < project config scalar values < explicit CLI flags.
    //
    // We implement the profile rung HERE by folding the resolved profile into a
    // local clone of `defaults` BEFORE the existing flag-vs-default resolution
    // below. `apply_profile` only fills fields config left unset (so a config
    // scalar still beats the profile), and the flag-vs-default logic below still
    // lets an explicit `--provider`/`--tool-set`/`--permission-mode`/etc. win
    // over the profile. With no profile selected, `defaults` is unchanged and
    // resolution is identical to before.
    let active_profile = profile_flag.or_else(|| defaults.profile.clone());
    let folded;
    let defaults: &AgentSettings = match &active_profile {
        Some(name) => {
            let profile = defaults.resolve_profile(name)?;
            let mut clone = defaults.clone();
            clone.apply_profile(&profile);
            folded = clone;
            &folded
        }
        None => defaults,
    };

    let provider = provider
        .or_else(|| defaults.provider.clone())
        .ok_or_else(|| {
            "no provider configured — run `codel00p providers use <id>` or pass --provider"
                .to_string()
        })?;
    let model = model.or_else(|| defaults.model.clone()).ok_or_else(|| {
        "no model configured — run `codel00p providers use <id> --model <model>` or pass --model"
            .to_string()
    })?;
    let provider_policy_preset =
        provider_policy_preset.or_else(|| defaults.provider_policy_preset.clone());
    let base_url = base_url.or_else(|| defaults.base_url.clone());
    let max_iterations = max_iterations.or(defaults.max_iterations);
    let permission_mode = match permission_mode {
        Some(mode) => mode,
        None => match &defaults.permission_mode {
            Some(value) => parse_permission_mode(value)?,
            None => CliPermissionMode::Allow,
        },
    };
    let tool_sets = if tool_sets.is_empty() {
        match &defaults.tool_sets {
            Some(values) => values
                .iter()
                .map(|value| parse_agent_tool_set(value))
                .collect::<CliResult<Vec<_>>>()?,
            // codel00p is a coding agent: an interactive `agent run`/`agent chat`
            // with no `--tool-set` (and no configured default) is fully capable —
            // it can read, edit, run commands, use git and the web, run pipelines
            // and `execute_code`, delegate to sub-agents, and propose skills. The
            // user never has to opt into capability with `--tool-set`; the flag is
            // only for *restricting* the agent (e.g. `--tool-set read`). `All`
            // expands to edit + command + git + web (and enables pipeline + code);
            // `Delegate` and `Learn` are added explicitly since they are wired
            // independently. The restricted unattended paths (gateway/cron) build
            // their own tool sets directly and are unaffected.
            None => vec![
                AgentToolSet::All,
                AgentToolSet::Delegate,
                AgentToolSet::Learn,
            ],
        }
    } else {
        tool_sets
    };
    // A flag wins; otherwise fall back to a configured default; otherwise leave
    // unset so the default CLI path forwards nothing to the provider.
    let tool_choice = match tool_choice {
        Some(choice) => Some(choice),
        None => match &defaults.tool_choice {
            Some(value) => Some(parse_tool_choice(value)?),
            None => None,
        },
    };
    let response_format = match response_format {
        Some(format) => Some(format),
        None => match &defaults.response_format {
            Some(value) => Some(parse_response_format(value)?),
            None => None,
        },
    };
    let stream = stream.or(defaults.stream).unwrap_or(false);
    let remember_permissions = remember_permissions
        .or(defaults.remember_permissions)
        .unwrap_or(false);
    // Explicit `--fallback` flags win; otherwise fall back to the configured
    // `agent.fallbacks` list. Empty in both cases leaves behavior unchanged.
    let fallback_routes = if fallback_routes.is_empty() {
        match &defaults.fallbacks {
            Some(values) => values
                .iter()
                .map(|value| parse_fallback_route(value))
                .collect::<CliResult<Vec<_>>>()?,
            None => Vec::new(),
        }
    } else {
        fallback_routes
    };

    Ok(AgentRunOptions {
        prompt: String::new(),
        workspace,
        provider,
        model,
        provider_policy_preset,
        base_url,
        session_id,
        max_iterations,
        json_events,
        stream_events,
        stream,
        tool_sets,
        tool_choice,
        response_format,
        permission_mode,
        remember_permissions,
        mcp_servers,
        fallback_routes,
        gateway_approval: None,
        // Interactive `agent run`/`chat`/`resume`/`continue` runs with an
        // operator present; only gateway/cron turns set this.
        unattended: false,
        profile: active_profile,
    })
}

pub(super) fn parse_agent_resume_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    if args.len() < 2 {
        return Err("usage: agent resume <session-id> <prompt>".to_string());
    }

    let session_id = args[0].clone();
    let mut options = parse_agent_run_options(defaults, &args[1..])?;
    options.session_id = Some(session_id);
    Ok(options)
}

pub(super) fn parse_agent_tool_set(value: &str) -> CliResult<AgentToolSet> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" | "read-only" | "readonly" => Ok(AgentToolSet::Read),
        "edit" | "editing" | "write" => Ok(AgentToolSet::Edit),
        "command" | "commands" | "shell" => Ok(AgentToolSet::Command),
        "git" => Ok(AgentToolSet::Git),
        "web" => Ok(AgentToolSet::Web),
        "delegate" | "delegation" => Ok(AgentToolSet::Delegate),
        "learn" | "learning" => Ok(AgentToolSet::Learn),
        "pipeline" | "programmatic" => Ok(AgentToolSet::Pipeline),
        "code" | "execute" | "code-execution" => Ok(AgentToolSet::Code),
        "all" => Ok(AgentToolSet::All),
        _ => Err(format!("unknown tool set: {value}")),
    }
}

/// Parses a `--fallback` value into an [`InferenceFallbackRoute`].
///
/// Format: `<provider>:<model>[@<base_url>]`. The provider/model split is on the
/// *first* `:` so model ids may contain further colons or slashes (e.g.
/// `anthropic/claude-sonnet`). An optional `@<base_url>` suffix supplies the
/// route's base URL — required for the `custom` provider, mirroring how the
/// primary route resolves a base URL. The base URL is split off first so a URL's
/// own `:` (e.g. `http://host:8080`) does not confuse the provider/model split.
pub(super) fn parse_fallback_route(value: &str) -> CliResult<InferenceFallbackRoute> {
    let (route, base_url) = match value.split_once('@') {
        Some((route, base_url)) => (route, Some(base_url.trim())),
        None => (value, None),
    };
    let (provider, model) = route.split_once(':').ok_or_else(|| {
        format!("invalid --fallback `{value}`: expected `<provider>:<model>[@<base_url>]`")
    })?;
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return Err(format!(
            "invalid --fallback `{value}`: provider and model must be non-empty"
        ));
    }
    let route = InferenceFallbackRoute::new(provider, model);
    Ok(match base_url {
        Some(base_url) if !base_url.is_empty() => route.base_url(base_url),
        _ => route,
    })
}

/// Resolves the configured `agent.fallbacks` list into routes. Used by the
/// unattended (cron/gateway) entrypoints that build options directly from
/// settings without a flag parser. Empty/absent yields no routes.
pub(super) fn resolve_configured_fallback_routes(
    fallbacks: Option<&Vec<String>>,
) -> CliResult<Vec<InferenceFallbackRoute>> {
    match fallbacks {
        Some(values) => values
            .iter()
            .map(|value| parse_fallback_route(value))
            .collect(),
        None => Ok(Vec::new()),
    }
}

pub(super) fn parse_permission_mode(value: &str) -> CliResult<CliPermissionMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" | "allowed" => Ok(CliPermissionMode::Allow),
        "ask" | "prompt" | "interactive" => Ok(CliPermissionMode::Ask),
        "deny" | "denied" => Ok(CliPermissionMode::Deny),
        _ => Err(format!("unknown permission mode: {value}")),
    }
}

/// Parses `--tool-choice <auto|required|none|NAME>`. A bare token that is not one
/// of the reserved words selects that specific tool by name
/// ([`ToolChoice::Tool`]).
pub(super) fn parse_tool_choice(value: &str) -> CliResult<codel00p_harness::ToolChoice> {
    use codel00p_harness::ToolChoice;
    let trimmed = value.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "" => Err("empty --tool-choice".to_string()),
        "auto" => Ok(ToolChoice::Auto),
        "required" | "any" => Ok(ToolChoice::Required),
        "none" => Ok(ToolChoice::None),
        // Any other token is a specific tool name; keep the original casing.
        _ => Ok(ToolChoice::Tool(trimmed.to_string())),
    }
}

/// Parses `--response-format <text|json>`. `json` maps to
/// [`ResponseFormat::JsonObject`] (JSON mode). The richer
/// [`ResponseFormat::JsonSchema`] variant takes a name + JSON Schema, which is
/// awkward to express on the command line, so it stays builder-only.
pub(super) fn parse_response_format(value: &str) -> CliResult<codel00p_harness::ResponseFormat> {
    use codel00p_harness::ResponseFormat;
    match value.trim().to_ascii_lowercase().as_str() {
        "" => Err("empty --response-format".to_string()),
        "text" => Ok(ResponseFormat::Text),
        "json" | "json_object" | "json-object" => Ok(ResponseFormat::JsonObject),
        _ => Err(format!("unknown response format: {value}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AgentSettings;

    fn test_defaults() -> AgentSettings {
        // A provider/model must resolve for option parsing to succeed; the tool
        // set is intentionally left unset to exercise the built-in default.
        AgentSettings {
            provider: Some("custom".to_string()),
            model: Some("test-model".to_string()),
            ..AgentSettings::default()
        }
    }

    fn run_opts(args: &[&str]) -> AgentRunOptions {
        let defaults = test_defaults();
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_agent_run_options(&defaults, &args).expect("parse")
    }

    #[test]
    fn default_run_is_fully_capable() {
        // A run with no `--tool-set` and no config default is fully capable: `All`
        // (edit + command + git + web, and pipeline + code) plus Delegate + Learn.
        // The user never has to opt into capability.
        let options = run_opts(&["build the thing"]);
        for expected in [
            AgentToolSet::All,
            AgentToolSet::Delegate,
            AgentToolSet::Learn,
        ] {
            assert!(
                options.tool_sets.contains(&expected),
                "default interactive run must include {expected:?}, got {:?}",
                options.tool_sets
            );
        }
    }

    #[test]
    fn code_tool_set_aliases_parse() {
        for alias in ["code", "execute", "code-execution", "CODE"] {
            assert_eq!(parse_agent_tool_set(alias).unwrap(), AgentToolSet::Code);
        }
        let options = run_opts(&["script it", "--tool-set", "code"]);
        assert!(options.tool_sets.contains(&AgentToolSet::Code));
    }

    #[test]
    fn explicit_tool_set_still_overrides_the_default() {
        // Passing `--tool-set read` opts back into read-only and must not be
        // silently widened to editing.
        let options = run_opts(&["look around", "--tool-set", "read"]);
        assert_eq!(options.tool_sets, vec![AgentToolSet::Read]);
    }

    #[test]
    fn parse_tool_choice_keywords_and_specific_tool() {
        use codel00p_harness::ToolChoice;
        assert_eq!(parse_tool_choice("auto").unwrap(), ToolChoice::Auto);
        assert_eq!(parse_tool_choice("REQUIRED").unwrap(), ToolChoice::Required);
        assert_eq!(parse_tool_choice("none").unwrap(), ToolChoice::None);
        // A bare tool name selects that tool and preserves casing.
        assert_eq!(
            parse_tool_choice("read_file").unwrap(),
            ToolChoice::Tool("read_file".to_string())
        );
        assert!(parse_tool_choice("  ").is_err());
    }

    #[test]
    fn parse_response_format_text_and_json() {
        use codel00p_harness::ResponseFormat;
        assert_eq!(parse_response_format("text").unwrap(), ResponseFormat::Text);
        assert_eq!(
            parse_response_format("json").unwrap(),
            ResponseFormat::JsonObject
        );
        assert_eq!(
            parse_response_format("JSON_OBJECT").unwrap(),
            ResponseFormat::JsonObject
        );
        assert!(parse_response_format("yaml").is_err());
    }

    #[test]
    fn default_run_leaves_tool_choice_and_response_format_unset() {
        // The default CLI path must forward neither knob to the provider.
        let options = run_opts(&["do something"]);
        assert!(options.tool_choice.is_none());
        assert!(options.response_format.is_none());
    }

    #[test]
    fn flags_populate_tool_choice_and_response_format() {
        use codel00p_harness::{ResponseFormat, ToolChoice};
        let options = run_opts(&[
            "do something",
            "--tool-choice",
            "required",
            "--response-format",
            "json",
        ]);
        assert_eq!(options.tool_choice, Some(ToolChoice::Required));
        assert_eq!(options.response_format, Some(ResponseFormat::JsonObject));
    }

    #[test]
    fn configured_defaults_apply_for_tool_choice_and_response_format() {
        use codel00p_harness::{ResponseFormat, ToolChoice};
        let defaults = AgentSettings {
            tool_choice: Some("read_file".to_string()),
            response_format: Some("json".to_string()),
            ..test_defaults()
        };
        let args = vec!["do something".to_string()];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        assert_eq!(
            options.tool_choice,
            Some(ToolChoice::Tool("read_file".to_string()))
        );
        assert_eq!(options.response_format, Some(ResponseFormat::JsonObject));
    }

    #[test]
    fn explicit_flag_overrides_configured_default() {
        use codel00p_harness::ToolChoice;
        let defaults = AgentSettings {
            tool_choice: Some("auto".to_string()),
            ..test_defaults()
        };
        let args = vec![
            "do something".to_string(),
            "--tool-choice".to_string(),
            "required".to_string(),
        ];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        assert_eq!(options.tool_choice, Some(ToolChoice::Required));
    }

    #[test]
    fn parse_fallback_route_provider_model_only() {
        let route = parse_fallback_route("openrouter:anthropic/claude-sonnet").expect("parse");
        assert_eq!(route.provider, "openrouter");
        assert_eq!(route.model, "anthropic/claude-sonnet");
        assert_eq!(route.base_url, None);
    }

    #[test]
    fn parse_fallback_route_with_base_url_keeps_url_colons() {
        let route =
            parse_fallback_route("custom:local-model@http://127.0.0.1:8080").expect("parse");
        assert_eq!(route.provider, "custom");
        assert_eq!(route.model, "local-model");
        assert_eq!(route.base_url.as_deref(), Some("http://127.0.0.1:8080"));
    }

    #[test]
    fn parse_fallback_route_rejects_missing_model() {
        assert!(parse_fallback_route("openrouter").is_err());
        assert!(parse_fallback_route("openrouter:").is_err());
        assert!(parse_fallback_route(":model").is_err());
    }

    #[test]
    fn explicit_fallback_flag_is_parsed_into_a_route() {
        let options = run_opts(&[
            "do it",
            "--fallback",
            "custom:local-model@http://127.0.0.1:9000",
        ]);
        assert_eq!(options.fallback_routes.len(), 1);
        assert_eq!(options.fallback_routes[0].provider, "custom");
        assert_eq!(options.fallback_routes[0].model, "local-model");
        assert_eq!(
            options.fallback_routes[0].base_url.as_deref(),
            Some("http://127.0.0.1:9000")
        );
    }

    #[test]
    fn configured_fallbacks_apply_when_no_flag_is_passed() {
        let defaults = AgentSettings {
            fallbacks: Some(vec!["openrouter:anthropic/claude-sonnet".to_string()]),
            ..test_defaults()
        };
        let args = vec!["do it".to_string()];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        assert_eq!(options.fallback_routes.len(), 1);
        assert_eq!(options.fallback_routes[0].provider, "openrouter");
    }

    #[test]
    fn explicit_fallback_flag_overrides_configured_fallbacks() {
        let defaults = AgentSettings {
            fallbacks: Some(vec!["openrouter:anthropic/claude-sonnet".to_string()]),
            ..test_defaults()
        };
        let args = vec![
            "do it".to_string(),
            "--fallback".to_string(),
            "custom:local-model".to_string(),
        ];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        assert_eq!(options.fallback_routes.len(), 1);
        assert_eq!(options.fallback_routes[0].provider, "custom");
        assert_eq!(options.fallback_routes[0].model, "local-model");
    }

    #[test]
    fn no_fallback_configured_yields_empty_routes() {
        let options = run_opts(&["do it"]);
        assert!(options.fallback_routes.is_empty());
    }

    #[test]
    fn profile_flag_applies_the_bundle() {
        // `--profile autonomous` pulls in the preset's permission mode + tool set.
        let options = run_opts(&["go", "--profile", "autonomous"]);
        assert_eq!(options.permission_mode, CliPermissionMode::Allow);
        assert!(options.tool_sets.contains(&AgentToolSet::All));
        assert_eq!(options.profile.as_deref(), Some("autonomous"));
    }

    #[test]
    fn explicit_flag_overrides_the_profile() {
        // The `careful` preset sets ask mode; an explicit `--permission-mode allow`
        // still wins (flags beat the profile).
        let options = run_opts(&[
            "go",
            "--profile",
            "careful",
            "--permission-mode",
            "allow",
            "--tool-set",
            "read",
        ]);
        assert_eq!(options.permission_mode, CliPermissionMode::Allow);
        assert_eq!(options.tool_sets, vec![AgentToolSet::Read]);
        assert_eq!(options.profile.as_deref(), Some("careful"));
    }

    #[test]
    fn agent_profile_config_default_applies_without_a_flag() {
        let defaults = AgentSettings {
            profile: Some("careful".to_string()),
            ..test_defaults()
        };
        let args = vec!["go".to_string()];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        // The configured default profile (`careful`) is applied: ask mode.
        assert_eq!(options.permission_mode, CliPermissionMode::Ask);
        assert_eq!(options.profile.as_deref(), Some("careful"));
    }

    #[test]
    fn profile_flag_overrides_agent_profile_default() {
        let defaults = AgentSettings {
            profile: Some("careful".to_string()),
            ..test_defaults()
        };
        let args = vec![
            "go".to_string(),
            "--profile".to_string(),
            "manual".to_string(),
        ];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        // `--profile manual` overrides the `agent.profile = careful` default.
        assert_eq!(options.profile.as_deref(), Some("manual"));
        assert_eq!(
            options.tool_sets,
            vec![AgentToolSet::Read, AgentToolSet::Edit]
        );
    }

    #[test]
    fn unknown_profile_errors_listing_available() {
        let args = vec![
            "go".to_string(),
            "--profile".to_string(),
            "nope".to_string(),
        ];
        let Err(error) = parse_agent_run_options(&test_defaults(), &args) else {
            panic!("unknown profile must error");
        };
        assert!(error.contains("nope"));
        assert!(error.contains("autonomous"));
        assert!(error.contains("careful"));
        assert!(error.contains("manual"));
    }

    #[test]
    fn user_profile_shadows_a_same_named_preset() {
        use std::collections::BTreeMap;
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "careful".to_string(),
            crate::settings::ProfileSettings {
                tool_sets: Some(vec!["git".to_string()]),
                ..crate::settings::ProfileSettings::default()
            },
        );
        let defaults = AgentSettings {
            profiles,
            ..test_defaults()
        };
        let args = vec![
            "go".to_string(),
            "--profile".to_string(),
            "careful".to_string(),
        ];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        // The user `careful` profile (tool_sets=[git]) shadows the preset
        // (which would have left permission_mode=ask without touching tool_sets).
        assert_eq!(options.tool_sets, vec![AgentToolSet::Git]);
    }

    #[test]
    fn no_profile_leaves_resolution_unchanged() {
        // With no profile selected, the default run is identical to before.
        let options = run_opts(&["go"]);
        assert!(options.profile.is_none());
        assert_eq!(options.permission_mode, CliPermissionMode::Allow);
        assert!(options.tool_sets.contains(&AgentToolSet::All));
    }

    #[test]
    fn configured_default_tool_sets_take_precedence_over_the_builtin_default() {
        let defaults = AgentSettings {
            tool_sets: Some(vec!["read".to_string(), "git".to_string()]),
            ..test_defaults()
        };
        let args = vec!["do something".to_string()];
        let options = parse_agent_run_options(&defaults, &args).expect("parse");
        assert_eq!(
            options.tool_sets,
            vec![AgentToolSet::Read, AgentToolSet::Git]
        );
    }
}

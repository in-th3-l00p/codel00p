//! Agent harness construction and one-shot turn execution.

use super::*;

/// Resolve the configured `agent.execution_backend` to a concrete
/// [`TerminalBackend`]. `local` (or absent) yields [`LocalBackend`] (today's
/// behavior); `docker` builds a [`DockerBackend`] from the workspace path plus
/// the `[agent.docker]` config section; anything else is a clear error.
fn resolve_execution_backend(workspace: &Path) -> CliResult<Arc<dyn TerminalBackend>> {
    let agent = crate::settings::load_layered(workspace)
        .ok()
        .map(|resolved| resolved.merged.agent)
        .unwrap_or_default();
    backend_from_setting(
        agent.execution_backend.as_deref(),
        workspace,
        &agent.docker,
        &agent.ssh,
    )
}

/// Map a resolved `agent.execution_backend` value to a backend. Absent or
/// `local` yields [`LocalBackend`]; `docker` builds a [`DockerBackend`] for
/// `workspace` from `docker` settings; anything else is a clear error. Kept
/// dependency-light (workspace path + settings, no live config load) so it is
/// unit-testable.
fn backend_from_setting(
    value: Option<&str>,
    workspace: &Path,
    docker: &DockerSettings,
    ssh: &SshSettings,
) -> CliResult<Arc<dyn TerminalBackend>> {
    match value {
        // Root the local backend at the (canonicalized) workspace so it can also
        // serve workspace filesystem ops, not just command execution. Falls back
        // to the raw path if canonicalization fails (e.g. a not-yet-created dir);
        // `Workspace::new` performs the authoritative canonicalize/validate.
        None | Some("local") => {
            let root = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
            Ok(Arc::new(LocalBackend::rooted(root)))
        }
        Some("docker") => Ok(Arc::new(docker_backend_from_settings(workspace, docker)?)),
        Some("ssh") => Ok(Arc::new(ssh_backend_from_settings(workspace, ssh)?)),
        Some(other) => Err(format!(
            "unknown agent.execution_backend `{other}`: supported values are `local`, `docker`, and `ssh`"
        )),
    }
}

/// Build a [`DockerBackend`] from the `[agent.docker]` settings, applying the
/// harness defaults for anything unset and resolving the workspace to an
/// absolute path (Docker bind mounts require absolute host paths).
fn docker_backend_from_settings(
    workspace: &Path,
    docker: &DockerSettings,
) -> CliResult<DockerBackend> {
    let workspace_root = std::fs::canonicalize(workspace).map_err(|error| {
        format!(
            "docker backend: cannot resolve workspace path {}: {error}",
            workspace.display()
        )
    })?;

    let mut config = DockerConfig::default();
    if let Some(image) = &docker.image {
        config.image = image.clone();
    }
    if let Some(mount) = &docker.container_mount {
        config.container_mount = mount.into();
    }
    if docker.memory.is_some() {
        config.memory = docker.memory.clone();
    }
    if docker.cpus.is_some() {
        config.cpus = docker.cpus.clone();
    }
    if docker.network.is_some() {
        config.network = docker.network.clone();
    }
    if let Some(map_host_user) = docker.map_host_user {
        config.map_host_user = map_host_user;
    }
    if let Some(reuse_container) = docker.reuse_container {
        config.reuse_container = reuse_container;
    }

    Ok(DockerBackend::new(workspace_root, config))
}

/// Build an [`SshBackend`] from the `[agent.ssh]` settings. The backend maps
/// command cwds from the LOCAL workspace root (canonicalized so the mapping
/// matches the paths the tools resolve under it) onto the REMOTE workspace.
/// `host` and `workspace` are required; everything else is optional and may be
/// supplied by the user's `~/.ssh/config`.
fn ssh_backend_from_settings(workspace: &Path, ssh: &SshSettings) -> CliResult<SshBackend> {
    let host = ssh
        .host
        .clone()
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| {
            "ssh backend: `agent.ssh.host` is required (a hostname/IP or ~/.ssh/config alias). \
         Set it or use agent.execution_backend = \"local\"."
                .to_string()
        })?;
    let remote_workspace = ssh
        .workspace
        .clone()
        .filter(|ws| !ws.trim().is_empty())
        .ok_or_else(|| {
            "ssh backend: `agent.ssh.workspace` is required (the absolute path on the remote host \
             where the workspace lives). Set it or use agent.execution_backend = \"local\"."
                .to_string()
        })?;

    // The local workspace root the tools resolve cwds under. Canonicalize so the
    // backend's strip_prefix matches; fall back to the raw path if it cannot be
    // canonicalized (e.g. a not-yet-created dir).
    let local_root = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());

    let mut config = SshConfig::new(host, remote_workspace);
    config.user = ssh.user.clone();
    config.port = ssh.port;
    config.identity_file = ssh.identity_file.clone().map(Into::into);

    Ok(SshBackend::new(local_root, config))
}

/// Org-policy guard (#7): when `require_isolation` is set, an `unattended` turn
/// whose tool sets can execute shell commands must run on an isolating backend.
/// Fail-closed — returns an actionable error otherwise. A no-op for interactive
/// turns, when the policy is off, when the backend is already isolating, or when
/// the turn cannot run shell commands.
fn enforce_unattended_isolation(
    unattended: bool,
    require_isolation: bool,
    tool_sets: &[AgentToolSet],
    backend_isolated: bool,
) -> CliResult<()> {
    if !unattended || !require_isolation || backend_isolated {
        return Ok(());
    }
    let shell_capable = tool_sets
        .iter()
        .any(|set| matches!(set, AgentToolSet::Command | AgentToolSet::All));
    if shell_capable {
        return Err(
            "agent.require_isolation_for_unattended is set: this unattended/gateway turn can \
             execute shell commands but the execution backend does not isolate them. Set \
             agent.execution_backend = \"docker\" (or disable the policy) to run it."
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    fn ssh_cfg() -> SshSettings {
        SshSettings {
            host: Some("example.com".into()),
            workspace: Some("/srv/ws".into()),
            ..SshSettings::default()
        }
    }

    #[test]
    fn execution_backend_resolves_local_docker_ssh_and_rejects_unknown() {
        let docker = DockerSettings::default();
        let ssh = ssh_cfg();
        // Absent and explicit `local` both resolve to the local backend.
        assert!(backend_from_setting(None, &ws(), &docker, &ssh).is_ok());
        assert!(backend_from_setting(Some("local"), &ws(), &docker, &ssh).is_ok());
        // `docker` and `ssh` are both valid backends now.
        assert!(backend_from_setting(Some("docker"), &ws(), &docker, &ssh).is_ok());
        assert!(backend_from_setting(Some("ssh"), &ws(), &docker, &ssh).is_ok());
        // Anything else is a clear, actionable error naming the supported set.
        let Err(error) = backend_from_setting(Some("podman"), &ws(), &docker, &ssh) else {
            panic!("expected unknown backend to error");
        };
        assert!(error.contains("podman"));
        assert!(error.contains("`local`"));
        assert!(error.contains("`docker`"));
        assert!(error.contains("`ssh`"));
    }

    #[test]
    fn ssh_backend_requires_host_and_workspace() {
        // Missing host: clear error naming the key.
        let no_host = SshSettings {
            host: None,
            workspace: Some("/srv/ws".into()),
            ..SshSettings::default()
        };
        let Err(error) = ssh_backend_from_settings(&ws(), &no_host) else {
            panic!("expected an error when host is missing");
        };
        assert!(error.contains("agent.ssh.host"));

        // Missing workspace: clear error naming the key.
        let no_ws = SshSettings {
            host: Some("example.com".into()),
            workspace: None,
            ..SshSettings::default()
        };
        let Err(error) = ssh_backend_from_settings(&ws(), &no_ws) else {
            panic!("expected an error when workspace is missing");
        };
        assert!(error.contains("agent.ssh.workspace"));

        // Both present: builds successfully and reports isolation.
        let backend = ssh_backend_from_settings(&ws(), &ssh_cfg()).unwrap();
        use codel00p_harness::TerminalBackend;
        assert!(backend.is_isolated());
    }

    #[test]
    fn ssh_settings_apply_to_config() {
        let ssh = SshSettings {
            host: Some("box".into()),
            user: Some("deploy".into()),
            port: Some(2222),
            identity_file: Some("/home/u/.ssh/id".into()),
            workspace: Some("/remote/ws".into()),
        };
        let backend = ssh_backend_from_settings(&ws(), &ssh).unwrap();
        let config = backend.config();
        assert_eq!(config.host, "box");
        assert_eq!(config.user.as_deref(), Some("deploy"));
        assert_eq!(config.port, Some(2222));
        assert_eq!(
            config.identity_file.as_deref(),
            Some(std::path::Path::new("/home/u/.ssh/id"))
        );
        assert_eq!(config.workspace, std::path::PathBuf::from("/remote/ws"));
    }

    #[test]
    fn unattended_shell_on_non_isolating_backend_is_refused() {
        // The policy bites only here: unattended + policy on + shell-capable +
        // non-isolating backend.
        let shell = [AgentToolSet::Read, AgentToolSet::Command];
        let Err(error) = enforce_unattended_isolation(true, true, &shell, false) else {
            panic!("expected the policy to refuse shell on a non-isolating backend");
        };
        assert!(error.contains("require_isolation_for_unattended"));
        assert!(error.contains("docker"));

        // `all` is shell-capable too.
        assert!(enforce_unattended_isolation(true, true, &[AgentToolSet::All], false).is_err());
    }

    #[test]
    fn isolation_policy_allows_the_safe_cases() {
        let shell = [AgentToolSet::Command];
        let readonly = [AgentToolSet::Read, AgentToolSet::Edit];
        // Isolating backend satisfies the policy.
        assert!(enforce_unattended_isolation(true, true, &shell, true).is_ok());
        // Interactive (attended) turns are never gated.
        assert!(enforce_unattended_isolation(false, true, &shell, false).is_ok());
        // Policy off ⇒ no gating.
        assert!(enforce_unattended_isolation(true, false, &shell, false).is_ok());
        // Unattended but not shell-capable ⇒ nothing to isolate.
        assert!(enforce_unattended_isolation(true, true, &readonly, false).is_ok());
    }

    #[test]
    fn docker_backend_reports_isolated_local_does_not() {
        use codel00p_harness::TerminalBackend;
        assert!(!LocalBackend::new().is_isolated());
        let docker = docker_backend_from_settings(&ws(), &DockerSettings::default()).unwrap();
        assert!(docker.is_isolated());
    }

    #[test]
    fn docker_settings_apply_over_defaults() {
        let docker = DockerSettings {
            image: Some("rust:1".into()),
            container_mount: Some("/src".into()),
            memory: Some("1g".into()),
            cpus: Some("2".into()),
            network: Some("bridge".into()),
            map_host_user: Some(false),
            reuse_container: Some(false),
        };
        let backend = docker_backend_from_settings(&ws(), &docker).unwrap();
        let expected = DockerConfig {
            image: "rust:1".into(),
            container_mount: "/src".into(),
            memory: Some("1g".into()),
            cpus: Some("2".into()),
            network: Some("bridge".into()),
            map_host_user: false,
            env: Vec::new(),
            reuse_container: false,
        };
        assert_eq!(backend.config(), &expected);
    }

    #[test]
    fn docker_defaults_when_settings_empty() {
        let backend = docker_backend_from_settings(&ws(), &DockerSettings::default()).unwrap();
        assert_eq!(backend.config(), &DockerConfig::default());
    }
}

pub(super) fn run_agent_turn(
    config: CliConfig,
    options: AgentRunOptions,
    session_mode: AgentSessionMode,
) -> CliResult<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut mcp_servers = load_mcp_servers_from_workspace(&options.workspace)?;
        mcp_servers.extend(options.mcp_servers.clone());

        let (session_state, previous_message_count) =
            prepare_session_state(&config, &options, session_mode)?;

        // Ctrl-C asks the turn to stop at the next boundary instead of killing the
        // process, so the partial session is still persisted below.
        let cancel = CancelSignal::new();
        let watcher_signal = cancel.clone();
        let watcher = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                watcher_signal.cancel();
            }
        });

        let harness = build_agent_harness_with(
            &config,
            &options,
            &mcp_servers,
            session_state.session_id(),
            None,
            cancel,
        )
        .await?;
        let outcome = harness
            .run_turn_with_state(session_state, UserMessage::new(options.prompt.clone()))
            .await
            .map_err(|error| {
                crate::error_help::humanize_provider_error(
                    &error.to_string(),
                    &options.provider,
                    &options.model,
                )
            })?;
        watcher.abort();

        let mut output = String::new();
        if let Some(message) = &outcome.assistant_message {
            if options.stream {
                // The assistant text was already streamed to stdout token by
                // token; just terminate the streamed line.
                output.push('\n');
            } else {
                output.push_str(message);
                output.push('\n');
            }
        }
        if options.json_events {
            for event in &outcome.events {
                output.push_str(&serde_json::to_string(&event).map_err(|error| error.to_string())?);
                output.push('\n');
            }
        }
        persist_turn_outcome(
            &config,
            &outcome.session_state,
            &outcome.events,
            previous_message_count,
        )?;

        // Surface the turn's token usage (and cost when priced) on stderr so it
        // is visible in normal runs without polluting stdout — which carries the
        // assistant reply and any `--json-events` NDJSON that scripts parse.
        if !options.json_events
            && let Some(summary) = turn_usage_summary(&outcome.events)
        {
            eprintln!("{summary}");
        }

        if outcome.cancelled {
            output
                .push_str("Interrupted — partial progress saved. Resume with `agent continue`.\n");
        }

        Ok(output)
    })
}

/// Render a concise one-line token-usage summary from the turn's events.
///
/// Returns `None` when the turn produced no usage data (e.g. a provider that
/// doesn't report it), so callers can stay silent rather than print zeros.
fn turn_usage_summary(events: &[AgentEvent]) -> Option<String> {
    let (usage, cost) = events.iter().rev().find_map(|event| match event {
        AgentEvent::TurnCompleted { usage, cost, .. } if usage.is_some() => {
            Some((usage.clone(), cost.clone()))
        }
        _ => None,
    })?;
    let usage = usage?;

    let mut line = format!(
        "tokens: {} prompt + {} completion = {} total",
        usage.prompt_tokens(),
        usage.completion_tokens(),
        usage.total_tokens(),
    );
    if let Some(cost) = cost
        && cost.total_nanos > 0
    {
        // Render nano-currency units as a fixed-point amount (1e9 nanos == 1 unit).
        let whole = cost.total_nanos / 1_000_000_000;
        let frac = cost.total_nanos % 1_000_000_000;
        line.push_str(&format!(
            " (cost: {whole}.{frac:09} {currency})",
            currency = cost.currency,
        ));
    }
    Some(line)
}

/// Builds a fresh `AgentHarness` from the parsed run options. The harness is
/// consumed by `run_turn_with_state`, so interactive chat rebuilds one per turn.
pub(super) async fn build_agent_harness(
    config: &CliConfig,
    options: &AgentRunOptions,
    mcp_servers: &[McpServerSpec],
    parent_session_id: &SessionId,
) -> CliResult<AgentHarness> {
    build_agent_harness_with(
        config,
        options,
        mcp_servers,
        parent_session_id,
        None,
        CancelSignal::new(),
    )
    .await
}

/// Async bridge used by the TUI to receive harness output without blocking the
/// render loop.
#[derive(Clone)]
pub(crate) struct UiBridge {
    pub(crate) tx: tokio::sync::mpsc::UnboundedSender<crate::tui::Msg>,
}

/// Builds an `AgentHarness`, optionally wiring its token/event/permission I/O to
/// a TUI channel instead of stdin/stdout.
pub(crate) async fn build_agent_harness_with(
    config: &CliConfig,
    options: &AgentRunOptions,
    mcp_servers: &[McpServerSpec],
    parent_session_id: &SessionId,
    ui_bridge: Option<UiBridge>,
    cancel: CancelSignal,
) -> CliResult<AgentHarness> {
    // Plugins are loaded once and contribute to providers, tools, and hooks.
    let plugins = load_plugins(&options.workspace)?;

    let provider_registry = plugins.apply_to_provider_registry(default_registry());
    let provider_client = build_provider_client_with(
        provider_registry.clone(),
        &options.provider,
        options.provider_policy_preset.as_deref(),
    )?;
    let model_client = ProviderModelClient::new(provider_client, &options.provider, &options.model);
    let model_client = if let Some(base_url) = &options.base_url {
        model_client.with_base_url(base_url)
    } else {
        model_client
    };
    // Attach any configured fallback routes so a fallback-eligible primary
    // failure transparently retries them. No-op when none are configured.
    let model_client = if options.fallback_routes.is_empty() {
        model_client
    } else {
        model_client.with_fallback_routes(options.fallback_routes.clone())
    };

    // Resolve where commands execute (`local` or an isolating `docker` backend)
    // and route BOTH the command tools and the workspace filesystem through it,
    // so file and command tools share one backend. Errors here surface an
    // unknown backend config.
    let execution_backend = resolve_execution_backend(&options.workspace)?;
    let workspace = Workspace::with_backend(&options.workspace, execution_backend.clone())
        .map_err(|error| error.to_string())?;
    let memory_provider = CliProjectMemoryProvider::new(config.clone()).with_limit(8);
    let memory_sink = CliMemoryCandidateSink::new(config.clone());
    let memory_extractor = ExplicitTurnMemoryExtractor::new(config.project.clone())
        .with_tag("agent")
        .with_tag("cli");

    // Org policy (#7): an unattended/gateway turn that can run shell commands
    // must do so on an isolating backend when the policy is enabled. Fail-closed.
    let require_isolation = crate::settings::load_layered(&options.workspace)
        .ok()
        .and_then(|resolved| resolved.merged.agent.require_isolation_for_unattended)
        .unwrap_or(false);
    enforce_unattended_isolation(
        options.unattended,
        require_isolation,
        &options.tool_sets,
        execution_backend.is_isolated(),
    )?;
    // Capture self-awareness facts that depend on the backend before it is moved
    // into `build_tool_registry`. The label mirrors the `agent.execution_backend`
    // setting (defaulting to `local`); isolation comes from the live backend.
    let backend_isolated = execution_backend.is_isolated();
    let backend_label = crate::settings::load_layered(&options.workspace)
        .ok()
        .and_then(|resolved| resolved.merged.agent.execution_backend)
        .unwrap_or_else(|| "local".to_string());
    let behavior = crate::settings::load_layered(&options.workspace)
        .ok()
        .map(|resolved| resolved.merged.agent.behavior)
        .unwrap_or_default();
    let mut tools = plugins.apply_to_tool_registry(
        build_tool_registry(&options.tool_sets, mcp_servers, execution_backend).await?,
    );
    // Back the always-present `update_plan` tool with a plan store the harness
    // also holds, so self-awareness run-state can report plan progress. This
    // replaces the internal store from `planning_defaults()` (last-writer-wins).
    let plan_store = PlanStore::new();
    tools = tools.with_tool(UpdatePlanTool::new(plan_store.clone()));
    if options.tool_sets.contains(&AgentToolSet::Delegate) {
        let max_children = crate::settings::load_layered(&options.workspace)
            .ok()
            .and_then(|resolved| resolved.merged.delegation.max_concurrent_children)
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MAX_CONCURRENT_CHILDREN);
        let spawner = Arc::new(CliSubAgentSpawner {
            config: config.clone(),
            parent_session_id: parent_session_id.clone(),
            workspace: options.workspace.clone(),
            provider_registry,
            provider: options.provider.clone(),
            model: options.model.clone(),
            base_url: options.base_url.clone(),
            policy_preset: options.provider_policy_preset.clone(),
            max_iterations: options
                .max_iterations
                .unwrap_or(CHILD_DEFAULT_MAX_ITERATIONS),
            concurrency: Arc::new(tokio::sync::Semaphore::new(max_children as usize)),
        });
        tools = tools.with_registry(delegation_tools(AgentRole::Orchestrator, spawner));
    }
    if options.tool_sets.contains(&AgentToolSet::Learn) {
        let sink = Arc::new(CliSkillProposalSink::new(crate::skills::user_skills_dir()));
        tools = tools.with_registry(learning_tools(sink));
    }
    // Shadow-git checkpoint tools ride with the git tool set (and `all`). They
    // need the codel00p home dir as the shadow-store base, which lives outside
    // the workspace, so they are folded in here rather than in `git_defaults()`.
    if options
        .tool_sets
        .iter()
        .any(|set| matches!(set, AgentToolSet::Git | AgentToolSet::All))
    {
        tools = tools.with_registry(checkpoint_tools(crate::settings::home_dir()));
    }

    let mut builder = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(tools);
    // A gateway turn routes privileged-tool permissions to a remote chat user's
    // `/approve` decision; everything else uses the local CLI permission mode.
    builder = match (&options.gateway_approval, &ui_bridge) {
        (Some(approval), _) => builder.permission_policy(GatewayApprovalPolicy::new(
            approval.store.clone(),
            approval.conversation.clone(),
            approval.granted_tool.clone(),
        )),
        (None, Some(ui)) => {
            builder.permission_policy(crate::tui::bridge::TuiPermissionPolicy::new(
                CliPermissionPolicy::new(
                    config.clone(),
                    options.permission_mode,
                    options.remember_permissions,
                ),
                ui.tx.clone(),
            ))
        }
        (None, None) => builder.permission_policy(CliPermissionPolicy::new(
            config.clone(),
            options.permission_mode,
            options.remember_permissions,
        )),
    };
    let mut builder = builder
        .project_memory_provider(memory_provider)
        .skill_provider(CliSkillProvider::new(crate::skills::skill_sources(
            &options.workspace,
        )))
        .turn_memory_extractor(memory_extractor)
        .memory_candidate_sink(memory_sink);
    if options.tool_sets.contains(&AgentToolSet::Learn) {
        // The agent can both explicitly propose skills (the tool above) and have
        // procedures auto-extracted from completed multi-step turns. Both land in
        // the same review queue.
        builder = builder
            .skill_extractor(ProcedureSkillExtractor::default())
            .skill_proposal_sink(CliSkillProposalSink::new(crate::skills::user_skills_dir()));
    }
    builder = plugins.apply_to_harness_builder(builder);
    if let Some(ui) = &ui_bridge {
        builder = builder
            .token_sink(crate::tui::bridge::ChannelTokenSink::new(ui.tx.clone()))
            .event_sink(crate::tui::bridge::ChannelEventSink::new(ui.tx.clone()));
    } else {
        if options.stream_events {
            builder = builder.event_sink(StdoutJsonEventSink);
        }
        if options.stream {
            builder = builder.token_sink(StdoutTokenSink);
        }
    }
    if let Some(max_iterations) = options.max_iterations {
        builder = builder.max_iterations(max_iterations);
    }
    if options
        .tool_sets
        .iter()
        .any(|set| matches!(set, AgentToolSet::Pipeline | AgentToolSet::All))
    {
        builder = builder.programmatic_tooling(true);
    }
    if options
        .tool_sets
        .iter()
        .any(|set| matches!(set, AgentToolSet::Code | AgentToolSet::All))
    {
        builder = builder.code_execution(true);
    }
    // Forward the CLI tool-choice / response-format knobs onto every turn when
    // set; left unset, the default CLI path stays unchanged (provider defaults).
    if let Some(tool_choice) = &options.tool_choice {
        builder = builder.tool_choice(tool_choice.clone());
    }
    if let Some(response_format) = &options.response_format {
        builder = builder.response_format(response_format.clone());
    }
    builder = builder.cancel_signal(cancel);

    // Self-awareness: build the static identity/capabilities context from the
    // resolved run config (never a hardcoded capability string — the tool sets,
    // backend, and permission mode are the real ones), apply the
    // `[agent.behavior]` toggles, and share the plan store so run-state can
    // report plan progress. Always installed so the `self_describe` tool is
    // available even with both toggles off (an explicit query still answers).
    let tool_set_labels: Vec<String> = options
        .tool_sets
        .iter()
        .map(|set| set.as_str().to_string())
        .collect();
    let self_context = AgentSelfContext::new(
        "codel00p",
        crate::update::current_version(),
        &options.provider,
        &options.model,
    )
    .with_tool_sets(tool_set_labels)
    .with_backend(backend_label, backend_isolated)
    .with_permission_mode(options.permission_mode.as_str())
    .with_profile(None)
    .with_toggles(
        behavior.self_knowledge_enabled(),
        behavior.self_state_enabled(),
    );
    builder = builder.agent_self(self_context).plan_store(plan_store);

    // Base operating prompt ("how I work"): default on, injected after the self
    // block and before project instructions. The planning guidance is included
    // unless `auto_plan` is off. With `base_prompt` off, no base block is added.
    if behavior.base_prompt_enabled() {
        builder = builder.base_prompt(codel00p_harness::base_prompt::base_prompt(
            behavior.auto_plan_enabled(),
        ));
    }

    // Verify-before-done + self-critique (the "perfect coding agent" core): when
    // the model signals done after a mutating turn, run the project's checks via
    // the registered `run_checks` tool and do not complete until they pass
    // (bounded by `verify_iterations`), then give one self-critique reflection
    // turn. All facets are individually toggleable under `[agent.behavior]`; the
    // user-facing defaults are on (except `lint_and_fix`). With `self_verify`
    // off, the harness behaves exactly as before.
    builder = builder.verify_config(codel00p_harness::VerifyConfig {
        self_verify: behavior.self_verify_enabled(),
        auto_test: behavior.auto_test_enabled(),
        lint_and_fix: behavior.lint_and_fix_enabled(),
        self_critique: behavior.self_critique_enabled(),
        verify_iterations: behavior.verify_iterations_value(),
        test_command: behavior.test_command_value(),
    });

    builder.build().map_err(|error| error.to_string())
}

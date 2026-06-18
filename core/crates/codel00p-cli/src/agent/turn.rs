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
    backend_from_setting(agent.execution_backend.as_deref(), workspace, &agent.docker)
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
) -> CliResult<Arc<dyn TerminalBackend>> {
    match value {
        None | Some("local") => Ok(Arc::new(LocalBackend::new())),
        Some("docker") => Ok(Arc::new(docker_backend_from_settings(workspace, docker)?)),
        Some(other) => Err(format!(
            "unknown agent.execution_backend `{other}`: supported values are `local` and `docker`"
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

    Ok(DockerBackend::new(workspace_root, config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn execution_backend_resolves_local_docker_and_rejects_unknown() {
        let docker = DockerSettings::default();
        // Absent and explicit `local` both resolve to the local backend.
        assert!(backend_from_setting(None, &ws(), &docker).is_ok());
        assert!(backend_from_setting(Some("local"), &ws(), &docker).is_ok());
        // `docker` is now a valid backend (previously an error in Phase 1).
        assert!(backend_from_setting(Some("docker"), &ws(), &docker).is_ok());
        // Anything else is a clear, actionable error naming the supported set.
        let Err(error) = backend_from_setting(Some("ssh"), &ws(), &docker) else {
            panic!("expected unknown backend to error");
        };
        assert!(error.contains("ssh"));
        assert!(error.contains("`local`"));
        assert!(error.contains("`docker`"));
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

    let workspace = Workspace::new(&options.workspace).map_err(|error| error.to_string())?;
    let memory_provider = CliProjectMemoryProvider::new(config.clone()).with_limit(8);
    let memory_sink = CliMemoryCandidateSink::new(config.clone());
    let memory_extractor = ExplicitTurnMemoryExtractor::new(config.project.clone())
        .with_tag("agent")
        .with_tag("cli");

    // Resolve where commands execute (Phase 1: always LocalBackend) and route the
    // command tools through it. Errors here surface an unknown backend config.
    let execution_backend = resolve_execution_backend(&options.workspace)?;
    let mut tools = plugins.apply_to_tool_registry(
        build_tool_registry(&options.tool_sets, mcp_servers, execution_backend).await?,
    );
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
    // Forward the CLI tool-choice / response-format knobs onto every turn when
    // set; left unset, the default CLI path stays unchanged (provider defaults).
    if let Some(tool_choice) = &options.tool_choice {
        builder = builder.tool_choice(tool_choice.clone());
    }
    if let Some(response_format) = &options.response_format {
        builder = builder.response_format(response_format.clone());
    }
    builder = builder.cancel_signal(cancel);

    builder.build().map_err(|error| error.to_string())
}

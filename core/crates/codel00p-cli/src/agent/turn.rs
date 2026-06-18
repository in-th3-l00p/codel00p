//! Agent harness construction and one-shot turn execution.

use super::*;

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

        if outcome.cancelled {
            output
                .push_str("Interrupted — partial progress saved. Resume with `agent continue`.\n");
        }

        Ok(output)
    })
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

    let workspace = Workspace::new(&options.workspace).map_err(|error| error.to_string())?;
    let memory_provider = CliProjectMemoryProvider::new(config.clone()).with_limit(8);
    let memory_sink = CliMemoryCandidateSink::new(config.clone());
    let memory_extractor = ExplicitTurnMemoryExtractor::new(config.project.clone())
        .with_tag("agent")
        .with_tag("cli");

    let mut tools =
        plugins.apply_to_tool_registry(build_tool_registry(&options.tool_sets, mcp_servers).await?);
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

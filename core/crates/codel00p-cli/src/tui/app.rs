//! The application model: all TUI state in one place, mutated only by `update`.

use codel00p_harness::SessionState;
use codel00p_protocol::{CostEstimate, TokenUsage, Viewer};
use tokio::sync::oneshot;

use crate::agent::{AgentRunOptions, McpServerSpec};
use crate::config::CliConfig;

use super::composer::Composer;
use super::conversation::Conversation;
use super::overlay::{ModelChoice, Overlay};
use super::theme::Theme;

/// Transcript scroll state. `offset_from_bottom` counts visual rows scrolled up
/// from the newest line; `follow` keeps the view pinned to the bottom as new
/// content streams in (a log tail). The renderer is the source of truth: it clamps
/// `offset_from_bottom` to the real wrapped height each frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollState {
    pub(crate) offset_from_bottom: u16,
    pub(crate) follow: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset_from_bottom: 0,
            follow: true,
        }
    }
}

/// Live status of the in-flight turn, for the status bar and spinner.
#[derive(Clone, Debug, Default)]
pub(crate) struct TurnStatus {
    pub(crate) running: bool,
    pub(crate) current_tool: Option<String>,
    pub(crate) iterations: u32,
    pub(crate) finish_reason: Option<String>,
    /// The `App::tick` value when the turn started, for the long-run charm.
    pub(crate) started_tick: u64,
}

/// Cumulative token usage for the current session, surfaced in the status bar.
///
/// The harness does not propagate provider `Usage` counters through `TurnOutcome`
/// or its event stream, so this is a content-length estimate (~4 chars / token)
/// recomputed from the session transcript after each turn. It is labeled as an
/// approximation in the status bar; it tracks growth, not exact billing.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SessionUsage {
    pub(crate) estimated_tokens: u64,
    pub(crate) messages: usize,
}

/// Read-only cloud context shown in the Org tab and status bar.
#[derive(Clone, Debug, Default)]
pub(crate) struct CloudState {
    pub(crate) viewer: Option<Viewer>,
    pub(crate) error: Option<String>,
    /// Whether stored credentials exist (so we know cloud calls can be attempted).
    pub(crate) configured: bool,
}

pub(crate) struct App {
    pub(crate) config: CliConfig,
    pub(crate) options: AgentRunOptions,
    pub(crate) mcp_servers: Vec<McpServerSpec>,
    pub(crate) session_state: SessionState,
    pub(crate) persisted_message_count: usize,
    pub(crate) conversation: Conversation,
    pub(crate) composer: Composer,
    /// Transcript scroll position (see [`ScrollState`]).
    pub(crate) scroll: ScrollState,
    /// The conversation viewport height from the last render, so paging keys can
    /// scroll by a screenful. Updated by `view::render`.
    pub(crate) viewport_rows: u16,
    pub(crate) overlay: Overlay,
    pub(crate) pending_permission: Option<oneshot::Sender<codel00p_harness::PermissionDecision>>,
    pub(crate) turn: TurnStatus,
    pub(crate) usage: SessionUsage,
    /// The most recent provider-reported token usage, captured from
    /// `InferenceCompleted`/`TurnCompleted` events. Preferred over the char-count
    /// estimate in [`SessionUsage`] when present; `None` until the first inference
    /// reports usage. Drives the advanced status bar's token + context meters.
    pub(crate) last_usage: Option<TokenUsage>,
    /// The most recent provider-reported cost estimate, captured from
    /// `InferenceCompleted`/`TurnCompleted` events. `None` until a provider
    /// reports pricing (many free/local models never do), in which case the cost
    /// HUD is omitted rather than showing a bogus `$0.00`.
    pub(crate) last_cost: Option<CostEstimate>,
    /// The latest verify-before-done / self-critique / failure-budget verdict, as
    /// a short status-bar line (e.g. "✓ verified" or "⚠ replanning"). `None`
    /// until the first such event arrives this session.
    pub(crate) verification: Option<String>,
    pub(crate) cloud: CloudState,
    pub(crate) theme: Theme,
    /// Show advanced status-bar info (model name, real tokens, context meter).
    /// Loaded from `tui.show_advanced` at startup; toggled in the Settings overlay.
    pub(crate) show_advanced: bool,
    /// Whether to run the background update check on startup. Loaded from
    /// `tui.check_updates` (default true) at startup; toggled in the Settings
    /// overlay. The check also requires the env kill switch to be unset.
    pub(crate) check_updates: bool,
    /// Whether the agent's self-knowledge block is injected each turn. Loaded
    /// from `agent.behavior.self_knowledge` (default true) at startup; toggled in
    /// the Settings overlay. Display + persistence only — the harness reads the
    /// persisted config when it builds each turn.
    pub(crate) self_knowledge: bool,
    /// Whether the agent's live run-state line is included. Loaded from
    /// `agent.behavior.self_state` (default true) at startup; toggled in the
    /// Settings overlay.
    pub(crate) self_state: bool,
    /// Whether the base operating prompt is injected each turn. Loaded from
    /// `agent.behavior.base_prompt` (default true) at startup; toggled in the
    /// Settings overlay. Display + persistence only — the harness reads the
    /// persisted config when it builds each turn.
    pub(crate) base_prompt: bool,
    /// Whether the base prompt's planning guidance is included. Loaded from
    /// `agent.behavior.auto_plan` (default true) at startup; toggled in the
    /// Settings overlay.
    pub(crate) auto_plan: bool,
    /// The agent loop's iteration ceiling. Loaded from `agent.max_iterations`
    /// (default 25) at startup; edited in the Advanced settings sub-overlay and
    /// persisted to the same config key the harness reads.
    pub(crate) max_iterations: u32,
    /// Max verify→fix attempts before completing as not-verified. Loaded from
    /// `agent.behavior.verify_iterations` (default 3); edited in Advanced.
    pub(crate) verify_iterations: u32,
    /// Consecutive same-operation failures before the replan nudge fires (0 =
    /// off). Loaded from `agent.behavior.failure_budget` (default 3); edited in
    /// Advanced.
    pub(crate) failure_budget: u32,
    /// The active agent profile name (`agent.profile`), or `None` when no profile
    /// is selected. Shown + switched in the Settings overlay; persisted to the
    /// same config key the harness reads on the next run.
    pub(crate) active_profile: Option<String>,
    /// All selectable profile names: built-in presets ∪ user-defined
    /// `[agent.profiles.*]`, sorted. Cycled by the Settings profile switcher.
    pub(crate) profile_names: Vec<String>,
    /// The tool-approval mode (`agent.permission_mode`): `allow` / `ask` / `deny`,
    /// or `None` for the built-in default. Cycled in the Settings overlay; set
    /// after construction in `run_async` (so it stays off `App::new`'s signature).
    pub(crate) permission_mode: Option<String>,
    /// The active local agent (multi-agent personas, #13), or `None` when the
    /// base home (the implicit `default` agent) is in use. Resolved from the
    /// sticky registry pointer at startup and updated on a live switch; shown in
    /// the header banner so the user always knows which memory is live.
    pub(crate) active_agent: Option<String>,
    /// The TRUE base home (`<os-home>/.codel00p` or the launch-time
    /// `CODEL00P_HOME`), captured ONCE at startup before any agent switch mutates
    /// the env. The registry's `base_home()` reads `CODEL00P_HOME` live, so after
    /// a switch points the env at an agent's home it would no longer return the
    /// base — we keep the captured value to resolve agent homes + the sticky
    /// pointer correctly across switches. Multi-agent personas, #13.
    pub(crate) base_home: std::path::PathBuf,
    pub(crate) should_quit: bool,
    pub(crate) tick: u64,
    /// A newer release version if one is already known (from the update cache or a
    /// live background check), shown as a header chip. Never blocks.
    pub(crate) update_available: Option<String>,
    /// Set once the user dismisses the update prompt this session, so a later
    /// cache read / re-entry doesn't reopen the panel repeatedly.
    pub(crate) update_prompt_dismissed: bool,
    /// Set when the user chose "Update now" in the update prompt. The event loop
    /// quits, restores the terminal, then runs the self-update — never while the
    /// TUI owns the terminal.
    pub(crate) run_update_on_exit: bool,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: CliConfig,
        options: AgentRunOptions,
        mcp_servers: Vec<McpServerSpec>,
        session_state: SessionState,
        persisted_message_count: usize,
        cloud_configured: bool,
        show_advanced: bool,
        check_updates: bool,
        self_knowledge: bool,
        self_state: bool,
        base_prompt: bool,
        auto_plan: bool,
        max_iterations: u32,
        verify_iterations: u32,
        failure_budget: u32,
        active_profile: Option<String>,
        profile_names: Vec<String>,
        active_agent: Option<String>,
        base_home: std::path::PathBuf,
    ) -> Self {
        Self {
            config,
            options,
            mcp_servers,
            session_state,
            persisted_message_count,
            conversation: Conversation::default(),
            composer: Composer::default(),
            scroll: ScrollState::default(),
            viewport_rows: 0,
            overlay: Overlay::None,
            pending_permission: None,
            turn: TurnStatus::default(),
            usage: SessionUsage::default(),
            last_usage: None,
            last_cost: None,
            verification: None,
            cloud: CloudState {
                configured: cloud_configured,
                ..CloudState::default()
            },
            theme: Theme::default(),
            show_advanced,
            check_updates,
            self_knowledge,
            self_state,
            base_prompt,
            auto_plan,
            max_iterations,
            verify_iterations,
            failure_budget,
            active_profile,
            profile_names,
            permission_mode: None,
            active_agent,
            base_home,
            should_quit: false,
            tick: 0,
            update_available: crate::update::cached_newer_version(),
            update_prompt_dismissed: false,
            run_update_on_exit: false,
        }
    }

    pub(crate) fn session_label(&self) -> String {
        self.session_state.session_id().as_str().to_string()
    }

    /// Recomputes the cumulative usage estimate from the current session transcript.
    /// Called after each turn finishes and after a session resume, so the status-bar
    /// meter reflects the live conversation. ~4 characters per token is the usual
    /// rough rule of thumb; see [`SessionUsage`] for why this is an estimate.
    pub(crate) fn refresh_usage(&mut self) {
        let messages = self.session_state.messages();
        let chars: usize = messages.iter().map(|message| message.content().len()).sum();
        self.usage = SessionUsage {
            estimated_tokens: (chars as u64).div_ceil(4),
            messages: messages.len(),
        };
    }

    /// Builds the model picker choices: the active model first (marked current),
    /// then a curated catalog of well-known models. Selecting any sets the model
    /// for later turns; a free-text path is offered via the picker filter + Enter
    /// on a catalog row, mirroring the CLI's unchecked `/model <id>`.
    pub(crate) fn model_choices(&self) -> Vec<ModelChoice> {
        let mut choices = vec![ModelChoice {
            provider: self.options.provider.clone(),
            model: self.options.model.clone(),
            note: Some("current".to_string()),
        }];
        for (provider, model) in MODEL_CATALOG {
            if *provider == self.options.provider && *model == self.options.model {
                continue;
            }
            choices.push(ModelChoice {
                provider: (*provider).to_string(),
                model: (*model).to_string(),
                note: None,
            });
        }
        choices
    }
}

/// Best-effort context-window lookup for known models, used to render the
/// `ctx used/size` meter in the advanced status bar. This is a static, hand-
/// maintained table (there is no per-model window in [`MODEL_CATALOG`]); it is
/// keyed by `(provider, model)` and returns the window size in tokens. Returns
/// `None` for anything not listed, in which case the meter shows just the used
/// count without a percentage. Provider-neutral and intentionally conservative.
pub(crate) fn context_window(provider: &str, model: &str) -> Option<u32> {
    // Normalize an OpenRouter-style `vendor/model` id to its bare model name so
    // routes like `anthropic/claude-opus-4-8` match the same window.
    let bare = model.rsplit('/').next().unwrap_or(model);
    let key = bare.to_ascii_lowercase();
    let window = match key.as_str() {
        // Anthropic Claude family.
        "claude-opus-4-8" | "claude-sonnet-4-6" | "claude-haiku-4-5" => 200_000,
        // OpenAI family.
        "gpt-4o" | "gpt-4o-mini" => 128_000,
        "o3" => 200_000,
        // Google Gemini family.
        "gemini-2.0-flash" => 1_000_000,
        "gemini-1.5-pro" => 2_000_000,
        _ => return None,
    };
    // `provider` is accepted for future provider-specific disambiguation; today
    // the bare model name is sufficient for the catalog entries.
    let _ = provider;
    Some(window)
}

/// A small, hand-maintained catalog of common models per provider. Suggestions
/// only — any model id remains reachable by switching providers via config.
const MODEL_CATALOG: &[(&str, &str)] = &[
    ("anthropic", "claude-opus-4-8"),
    ("anthropic", "claude-sonnet-4-6"),
    ("anthropic", "claude-haiku-4-5"),
    ("openai", "gpt-4o"),
    ("openai", "gpt-4o-mini"),
    ("openai", "o3"),
    ("gemini", "gemini-2.0-flash"),
    ("gemini", "gemini-1.5-pro"),
    ("openrouter", "anthropic/claude-opus-4-8"),
    ("openrouter", "openai/gpt-4o"),
];

#[cfg(test)]
mod tests {
    use super::context_window;

    #[test]
    fn context_window_returns_known_and_unknown() {
        // Known catalog models resolve to their window.
        assert_eq!(
            context_window("anthropic", "claude-opus-4-8"),
            Some(200_000)
        );
        assert_eq!(context_window("openai", "gpt-4o"), Some(128_000));
        assert_eq!(context_window("gemini", "gemini-1.5-pro"), Some(2_000_000));
        // OpenRouter-style `vendor/model` ids normalize to the bare model.
        assert_eq!(
            context_window("openrouter", "anthropic/claude-opus-4-8"),
            Some(200_000)
        );
        // Unknown models return None so the meter shows "unknown" size.
        assert_eq!(context_window("acme", "mystery-model-9000"), None);
    }
}

//! The application model: all TUI state in one place, mutated only by `update`.

use codel00p_harness::SessionState;
use codel00p_protocol::{TokenUsage, Viewer};
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
    pub(crate) cloud: CloudState,
    pub(crate) theme: Theme,
    /// Show advanced status-bar info (model name, real tokens, context meter).
    /// Loaded from `tui.show_advanced` at startup; toggled in the Settings overlay.
    pub(crate) show_advanced: bool,
    pub(crate) should_quit: bool,
    pub(crate) tick: u64,
    /// A newer release version if one is already known (from the update cache),
    /// shown as a header chip. Read once at startup; never blocks.
    pub(crate) update_available: Option<String>,
}

impl App {
    pub(crate) fn new(
        config: CliConfig,
        options: AgentRunOptions,
        mcp_servers: Vec<McpServerSpec>,
        session_state: SessionState,
        persisted_message_count: usize,
        cloud_configured: bool,
        show_advanced: bool,
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
            cloud: CloudState {
                configured: cloud_configured,
                ..CloudState::default()
            },
            theme: Theme::default(),
            show_advanced,
            should_quit: false,
            tick: 0,
            update_available: crate::update::cached_newer_version(),
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

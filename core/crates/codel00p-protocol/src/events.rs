//! Agent runtime events emitted by harness execution.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{EventId, PermissionScope, SessionId, TurnId};

/// Protocol-local mirror of normalized token usage counters.
///
/// Mirrors `codel00p_providers::Usage` as a plain serializable struct so the
/// protocol crate does not need to depend on the providers crate. All counts
/// are token totals reported by (or estimated from) the provider response.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    /// Total prompt-side tokens (input + cache reads/writes).
    pub fn prompt_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }

    /// Total completion-side tokens (output + reasoning).
    pub fn completion_tokens(&self) -> u64 {
        self.output_tokens.saturating_add(self.reasoning_tokens)
    }

    /// Grand total of all token counters.
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens()
            .saturating_add(self.completion_tokens())
    }

    /// Accumulate another usage record into this one (saturating).
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(other.reasoning_tokens);
    }
}

/// Protocol-local mirror of a normalized usage cost estimate.
///
/// Mirrors `codel00p_providers::UsageCostEstimate`. Costs are expressed in
/// nano units of `currency` (e.g. USD nanos: 1_000_000_000 == $1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub currency: String,
    pub total_nanos: u64,
}

impl CostEstimate {
    /// Accumulate another cost into this one (saturating). The currency of the
    /// accumulator is taken from the first non-empty currency seen.
    pub fn add(&mut self, other: &CostEstimate) {
        if self.currency.is_empty() {
            self.currency = other.currency.clone();
        }
        self.total_nanos = self.total_nanos.saturating_add(other.total_nanos);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        event_id: EventId,
        session_id: SessionId,
    },
    TurnStarted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
    },
    ContextBuilt {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        message_count: usize,
    },
    ContextManifest {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        /// Project-instruction files included, in load order.
        instruction_sources: Vec<String>,
        /// IDs of approved memory entries injected, sorted for determinism.
        injected_memory_ids: Vec<String>,
        /// Advertised tool names, sorted for determinism.
        advertised_tools: Vec<String>,
        /// Selected skill names, sorted for determinism.
        skill_names: Vec<String>,
        /// Session message count at context-build time.
        message_count: usize,
        /// SHA-256 hex digest of the manifest's semantic inputs (sorted vecs + counts).
        /// Identical context ⇒ identical hash; changes when any input changes.
        content_hash: String,
    },
    ContextCompacted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        before_message_count: usize,
        after_message_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    InferenceRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        provider: String,
        model: String,
    },
    InferenceCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        finish_reason: Option<String>,
        /// Token usage for this single inference, when the provider reported it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        /// Estimated cost for this single inference, when pricing was available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost: Option<CostEstimate>,
    },
    /// Incremental tool-call argument fragment streamed by the provider before
    /// the call is fully assembled. Emitted only while streaming, and only by
    /// transports that stream tool calls; it is additive and never replaces the
    /// final [`AgentEvent::ToolCallRequested`]/`ToolCallCompleted` events, which
    /// still describe the assembled call. Consumers that do not care about live
    /// assembly can ignore it.
    ToolCallArgsDelta {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        /// Position of the tool call within the streamed response (several may
        /// be built in parallel).
        index: usize,
        /// Provider-assigned call id, when this fragment carried one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Tool name, when this fragment carried one (typically the first
        /// fragment of a given call).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Raw JSON-argument text for this fragment (possibly empty when only
        /// `id`/`name` arrived).
        args_fragment: String,
    },
    ToolCallRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        message: String,
    },
    PermissionRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        scope: PermissionScope,
    },
    PermissionDenied {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        message: String,
    },
    ToolProgress {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    LifecycleHookFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        hook: String,
        message: String,
    },
    TurnCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        iterations: u32,
        /// Aggregated token usage across every inference in the turn, when any
        /// inference reported usage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        /// Aggregated estimated cost across the turn, when pricing was available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost: Option<CostEstimate>,
    },
}

impl AgentEvent {
    pub fn tool_call_completed(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
    ) -> Self {
        Self::ToolCallCompleted {
            event_id,
            session_id,
            turn_id,
            tool_name: tool_name.into(),
        }
    }

    /// Build a [`AgentEvent::ContextManifest`] from its semantic inputs.
    ///
    /// The `content_hash` is computed as the SHA-256 of a canonical text
    /// representation of the sorted vecs and counts, so identical context
    /// always produces the identical hash.
    #[allow(clippy::too_many_arguments)]
    pub fn context_manifest(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        instruction_sources: Vec<String>,
        mut injected_memory_ids: Vec<String>,
        mut advertised_tools: Vec<String>,
        mut skill_names: Vec<String>,
        message_count: usize,
    ) -> Self {
        // Normalise — sort everything that should be sorted for determinism.
        // instruction_sources intentionally keeps load order (already deterministic).
        injected_memory_ids.sort();
        advertised_tools.sort();
        skill_names.sort();
        // instruction_sources: preserve load order (deterministic by definition).

        let content_hash = compute_manifest_hash(
            &instruction_sources,
            &injected_memory_ids,
            &advertised_tools,
            &skill_names,
            message_count,
        );

        Self::ContextManifest {
            event_id,
            session_id,
            turn_id,
            instruction_sources,
            injected_memory_ids,
            advertised_tools,
            skill_names,
            message_count,
            content_hash,
        }
    }
}

/// Compute a deterministic SHA-256 hex hash over the manifest's semantic inputs.
///
/// The canonical form is newline-separated sections, each section a
/// header followed by sorted, newline-separated values, then the
/// message count. This encoding is stable: adding or removing a field
/// requires a deliberate change here, and identical inputs always
/// produce the identical digest.
pub fn compute_manifest_hash(
    instruction_sources: &[String],
    injected_memory_ids: &[String],
    advertised_tools: &[String],
    skill_names: &[String],
    message_count: usize,
) -> String {
    let mut canonical = String::new();
    canonical.push_str("instruction_sources:");
    for s in instruction_sources {
        canonical.push('\n');
        canonical.push_str(s);
    }
    canonical.push_str("\ninjected_memory_ids:");
    for s in injected_memory_ids {
        canonical.push('\n');
        canonical.push_str(s);
    }
    canonical.push_str("\nadvertised_tools:");
    for s in advertised_tools {
        canonical.push('\n');
        canonical.push_str(s);
    }
    canonical.push_str("\nskill_names:");
    for s in skill_names {
        canonical.push('\n');
        canonical.push_str(s);
    }
    canonical.push_str("\nmessage_count:");
    canonical.push('\n');
    canonical.push_str(&message_count.to_string());

    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_manifest_serialization_round_trip() {
        let event = AgentEvent::context_manifest(
            EventId::from_static("ev-1"),
            SessionId::from_static("ses-1"),
            TurnId::from_static("turn-1"),
            vec!["CODEL00P.md".to_string()],
            vec!["mem-b".to_string(), "mem-a".to_string()],
            vec!["grep".to_string(), "find_files".to_string()],
            vec!["deploy".to_string()],
            3,
        );

        let json = serde_json::to_string(&event).expect("serialize");
        let back: AgentEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn context_manifest_hash_is_stable_for_identical_inputs() {
        let hash1 = compute_manifest_hash(
            &["CODEL00P.md".to_string()],
            &["mem-a".to_string()],
            &["grep".to_string()],
            &["deploy".to_string()],
            3,
        );
        let hash2 = compute_manifest_hash(
            &["CODEL00P.md".to_string()],
            &["mem-a".to_string()],
            &["grep".to_string()],
            &["deploy".to_string()],
            3,
        );
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64, "SHA-256 hex is 64 chars");
    }

    #[test]
    fn inference_completed_usage_round_trip() {
        let event = AgentEvent::InferenceCompleted {
            event_id: EventId::from_static("ev-1"),
            session_id: SessionId::from_static("ses-1"),
            turn_id: TurnId::from_static("turn-1"),
            finish_reason: Some("stop".to_string()),
            usage: Some(TokenUsage {
                input_tokens: 1234,
                output_tokens: 567,
                cache_read_tokens: 10,
                cache_write_tokens: 0,
                reasoning_tokens: 5,
            }),
            cost: Some(CostEstimate {
                currency: "USD".to_string(),
                total_nanos: 4_200_000,
            }),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let back: AgentEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn turn_completed_usage_round_trip() {
        let event = AgentEvent::TurnCompleted {
            event_id: EventId::from_static("ev-1"),
            session_id: SessionId::from_static("ses-1"),
            turn_id: TurnId::from_static("turn-1"),
            iterations: 3,
            usage: Some(TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            }),
            cost: None,
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let back: AgentEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn events_without_usage_deserialize_for_backward_compat() {
        // Events serialized before usage/cost existed omit those keys entirely.
        let inference = r#"{"kind":"inference_completed","event_id":"ev-1","session_id":"ses-1","turn_id":"turn-1","finish_reason":"stop"}"#;
        let event: AgentEvent = serde_json::from_str(inference).expect("deserialize legacy");
        assert!(matches!(
            event,
            AgentEvent::InferenceCompleted {
                usage: None,
                cost: None,
                ..
            }
        ));

        let turn = r#"{"kind":"turn_completed","event_id":"ev-1","session_id":"ses-1","turn_id":"turn-1","iterations":1}"#;
        let event: AgentEvent = serde_json::from_str(turn).expect("deserialize legacy");
        assert!(matches!(
            event,
            AgentEvent::TurnCompleted {
                usage: None,
                cost: None,
                ..
            }
        ));
    }

    #[test]
    fn tool_call_args_delta_round_trip() {
        let event = AgentEvent::ToolCallArgsDelta {
            event_id: EventId::from_static("ev-1"),
            session_id: SessionId::from_static("ses-1"),
            turn_id: TurnId::from_static("turn-1"),
            index: 0,
            id: Some("call_abc".to_string()),
            name: Some("grep".to_string()),
            args_fragment: "{\"pattern\":".to_string(),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"kind\":\"tool_call_args_delta\""));
        let back: AgentEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn tool_call_args_delta_omits_absent_id_and_name() {
        // Argument-only fragments (no id/name) skip those keys entirely.
        let event = AgentEvent::ToolCallArgsDelta {
            event_id: EventId::from_static("ev-1"),
            session_id: SessionId::from_static("ses-1"),
            turn_id: TurnId::from_static("turn-1"),
            index: 1,
            id: None,
            name: None,
            args_fragment: "\"value\"}".to_string(),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"name\""));
        let back: AgentEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn token_usage_aggregates_and_totals() {
        let mut total = TokenUsage::default();
        total.add(&TokenUsage {
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 5,
            reasoning_tokens: 3,
            ..Default::default()
        });
        total.add(&TokenUsage {
            input_tokens: 200,
            output_tokens: 40,
            ..Default::default()
        });
        assert_eq!(total.input_tokens, 300);
        assert_eq!(total.output_tokens, 60);
        assert_eq!(total.prompt_tokens(), 305);
        assert_eq!(total.completion_tokens(), 63);
        assert_eq!(total.total_tokens(), 368);
    }

    #[test]
    fn context_manifest_hash_changes_when_memory_changes() {
        let hash_a = compute_manifest_hash(
            &["CODEL00P.md".to_string()],
            &["mem-a".to_string()],
            &["grep".to_string()],
            &[],
            3,
        );
        let hash_b = compute_manifest_hash(
            &["CODEL00P.md".to_string()],
            &["mem-b".to_string()],
            &["grep".to_string()],
            &[],
            3,
        );
        assert_ne!(hash_a, hash_b);
    }
}

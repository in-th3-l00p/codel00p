//! Agent runtime events emitted by harness execution.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{EventId, PermissionScope, SessionId, TurnId};

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

//! Integration tests for the deterministic context manifest event.
//!
//! Mirrors the style of `agent_turn.rs`: uses `ScriptedModelClient` and a
//! `RecordingEventSink` to run a turn and then asserts on the emitted
//! `ContextManifest` event.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    AgentEventSink, AgentHarness, HarnessError, HarnessEvent, HarnessInferenceResponse,
    MemoryRepositoryProjectMemoryProvider, SessionId, SkillContext, SkillPrompt, SkillProvider,
    SkillSelectionRequest, ToolRegistry, UserMessage, Workspace,
};
use codel00p_memory::{
    InMemoryMemoryStore, MemoryCandidateInput, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemorySource, ProjectRef, TurnId, compute_manifest_hash};
use support::ScriptedModelClient;
use tempfile::tempdir;

// ─── Helpers ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct RecordingEventSink {
    events: Arc<Mutex<Vec<HarnessEvent>>>,
}

impl RecordingEventSink {
    fn events(&self) -> Vec<HarnessEvent> {
        self.events.lock().expect("events lock").clone()
    }
}

#[async_trait]
impl AgentEventSink for RecordingEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        self.events.lock().expect("events lock").push(event.clone());
    }
}

/// A `SkillProvider` that always returns a fixed set of skills.
struct FixedSkillProvider {
    skills: Vec<SkillPrompt>,
}

#[async_trait]
impl SkillProvider for FixedSkillProvider {
    async fn select(&self, _request: SkillSelectionRequest) -> Result<SkillContext, HarnessError> {
        Ok(SkillContext::new(self.skills.clone()))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// A turn with project instructions + injected memory + tools emits a
/// `ContextManifest` event whose fields match the actual inputs.
#[tokio::test]
async fn context_manifest_event_emitted_with_correct_fields() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("CODEL00P.md"), "Always run pnpm verify.\n")
        .expect("write instructions");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let project = ProjectRef::new("project-1", "codel00p");
    let source = MemorySource::turn(
        SessionId::from_static("session-manifest-source"),
        TurnId::from_static("turn-manifest-source"),
    );
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-alpha",
                project.clone(),
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                source.clone(),
            )
            .with_tag("harness"),
        )
        .expect("create mem-alpha");
    store
        .review("mem-alpha", ReviewDecision::approve("alice"))
        .expect("approve mem-alpha");

    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);
    let sink = RecordingEventSink::default();
    let session_id = SessionId::from_static("session-context-manifest");

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .project_memory_provider(
            // Static filter-only retrieval: this test asserts manifest mechanics
            // (injected_memory_ids), not relevance ranking, so keep proactive off.
            MemoryRepositoryProjectMemoryProvider::new(project, store)
                .with_proactive(false)
                .with_kind(MemoryKind::Architecture)
                .with_tag("harness"),
        )
        .event_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(session_id.clone(), UserMessage::new("Inspect."))
        .await
        .expect("run turn");

    // There must be exactly one ContextManifest event in the outcome events.
    let manifest_events: Vec<_> = outcome
        .events
        .iter()
        .filter(|ev| matches!(ev, HarnessEvent::ContextManifest { .. }))
        .collect();
    assert_eq!(
        manifest_events.len(),
        1,
        "expected exactly one ContextManifest event"
    );

    match manifest_events[0] {
        HarnessEvent::ContextManifest {
            session_id: sid,
            instruction_sources,
            injected_memory_ids,
            advertised_tools,
            skill_names,
            message_count,
            content_hash,
            ..
        } => {
            assert_eq!(sid, &session_id);
            assert_eq!(instruction_sources, &["CODEL00P.md"]);
            assert_eq!(injected_memory_ids, &["mem-alpha"]);
            // ToolRegistry::read_only_defaults() advertises these tools (sorted):
            assert_eq!(
                advertised_tools,
                &[
                    "find_files",
                    "grep",
                    "list_files",
                    "read_file",
                    "repo_map",
                    "search_text"
                ]
            );
            assert!(skill_names.is_empty());
            assert_eq!(*message_count, 1);
            assert_eq!(content_hash.len(), 64, "SHA-256 hex hash is 64 chars");
        }
        _ => panic!("unexpected event type"),
    }

    // Event sink must have received the same events.
    assert_eq!(sink.events(), outcome.events);
}

/// Running two identical turns produces the same `content_hash`.
///
/// We verify hash stability using `compute_manifest_hash` directly rather than
/// running two full turns (which avoids borrowing a non-`Clone` store twice).
#[test]
fn content_hash_is_stable_across_two_identical_inputs() {
    let hash1 = compute_manifest_hash(
        &["CODEL00P.md".to_string()],
        &["mem-beta".to_string()],
        &[
            "find_files".to_string(),
            "grep".to_string(),
            "list_files".to_string(),
        ],
        &[],
        1,
    );
    let hash2 = compute_manifest_hash(
        &["CODEL00P.md".to_string()],
        &["mem-beta".to_string()],
        &[
            "find_files".to_string(),
            "grep".to_string(),
            "list_files".to_string(),
        ],
        &[],
        1,
    );
    assert_eq!(hash1, hash2, "identical inputs must produce identical hash");
    assert_eq!(hash1.len(), 64, "SHA-256 hex hash is 64 chars");
}

/// Changing the injected memory set changes the `content_hash`.
#[tokio::test]
async fn content_hash_changes_when_injected_memory_changes() {
    let workspace_a = {
        let d = tempdir().expect("tempdir");
        // no instruction files — isolate the variable to memory only
        Workspace::new(d.path()).expect("workspace")
    };
    let workspace_b = {
        let d = tempdir().expect("tempdir");
        Workspace::new(d.path()).expect("workspace")
    };

    // Build a helper that creates a harness with exactly one approved memory entry.
    let run_with_memory_id =
        |ws: Workspace, memory_id: &'static str, memory_content: &'static str| {
            let project = ProjectRef::new("project-hash-change", "codel00p");
            let source = MemorySource::turn(
                SessionId::from_static("session-hash-change-source"),
                TurnId::from_static("turn-hash-change-source"),
            );
            let mut store = InMemoryMemoryStore::default();
            store
                .create_candidate(MemoryCandidateInput::new(
                    memory_id,
                    project.clone(),
                    MemoryKind::Architecture,
                    memory_content,
                    source,
                ))
                .expect("create memory");
            store
                .review(memory_id, ReviewDecision::approve("alice"))
                .expect("approve memory");
            let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
                "github", "gpt-4o", "Done.",
            )]);
            async move {
                AgentHarness::builder()
                    .model_client(model)
                    .workspace(ws)
                    .tools(ToolRegistry::read_only_defaults())
                    .project_memory_provider(
                        MemoryRepositoryProjectMemoryProvider::new(project, store)
                            .with_proactive(false),
                    )
                    .build()
                    .expect("build harness")
                    .run_turn(
                        SessionId::from_static("session-hash-change"),
                        UserMessage::new("Check."),
                    )
                    .await
                    .expect("run turn")
            }
        };

    let outcome_a = run_with_memory_id(workspace_a, "mem-x", "Memory X.").await;
    let outcome_b = run_with_memory_id(workspace_b, "mem-y", "Memory Y.").await;

    let hash_of = |outcome: &codel00p_harness::TurnOutcome| {
        outcome
            .events
            .iter()
            .find_map(|ev| {
                if let HarnessEvent::ContextManifest { content_hash, .. } = ev {
                    Some(content_hash.clone())
                } else {
                    None
                }
            })
            .expect("ContextManifest event")
    };

    let hash_a = hash_of(&outcome_a);
    let hash_b = hash_of(&outcome_b);
    assert_ne!(
        hash_a, hash_b,
        "different memory sets must produce different hashes"
    );
}

/// A turn with selected skills includes them sorted in the manifest.
#[tokio::test]
async fn context_manifest_includes_sorted_skill_names() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .skill_provider(FixedSkillProvider {
            skills: vec![
                SkillPrompt::new("zebra-skill", "Do Z things."),
                SkillPrompt::new("alpha-skill", "Do A things."),
            ],
        })
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-skill-manifest"),
            UserMessage::new("Use skills."),
        )
        .await
        .expect("run turn");

    let manifest = outcome
        .events
        .iter()
        .find_map(|ev| {
            if let HarnessEvent::ContextManifest { skill_names, .. } = ev {
                Some(skill_names.clone())
            } else {
                None
            }
        })
        .expect("ContextManifest event");

    assert_eq!(
        manifest,
        vec!["alpha-skill", "zebra-skill"],
        "skill names must be sorted"
    );
}

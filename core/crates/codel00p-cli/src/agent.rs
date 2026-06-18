use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use codel00p_cron::CronJob;
use codel00p_gateway::{
    GatewayCommand,
    approvals::{ApprovalOutcome, ApprovalStore},
};
use codel00p_harness::{
    AgentEventSink, AgentHarness, AgentRole, CancelSignal, DelegatedTask, DelegationOutcome,
    ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent, MemoryCandidateSink,
    MemoryCandidateSinkOutcome, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope, ProcedureSkillExtractor, ProjectMemoryContext,
    ProjectMemoryItem, ProjectMemoryProvider, ProjectMemoryRequest, ProposedSkill,
    ProviderModelClient, SessionId, SkillContext, SkillPrompt, SkillProposalSink, SkillProvider,
    SkillSelectionRequest, SubAgentSpawner, TokenSink, ToolRegistry, UserMessage, Workspace,
    delegation_tools, learning_tools,
};
use codel00p_mcp::{
    HttpServerEndpoint, McpClient, McpHttpClient, McpStdioClient, McpTool, McpToolDescriptor,
    StdioServerCommand,
};
use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryQuery, MemoryRepository};
use codel00p_plugin::PluginRegistry;
use codel00p_protocol::AgentEvent;
use codel00p_providers::{InferenceFallbackRoute, ProviderRegistry, default_registry};
use codel00p_session::{SessionMetadata, SessionRecord, SessionStore, SessionStoreError};
use codel00p_skill::{
    SkillError, SkillProposal, SkillSource, load_skills, propose_skill, record_skill_usage,
    select_skills,
};

use crate::{
    config::{
        CliConfig, CliResult, open_memory_store, open_session_store, parse_session_id,
        required_value,
    },
    connector_permissions::{
        ConnectorPermissionDecision, ConnectorPermissionStatus, is_rememberable_permission,
        load_decision, remember_decision,
    },
    providers::build_provider_client_with,
    session::{session_message_summary, session_role_label},
    settings::AgentSettings,
};

mod chat;
mod command;
mod delegation;
mod events;
mod mcp;
mod memory;
mod options;
mod permissions;
mod plugins;
mod session_state;
mod skills;
mod tooling;
mod turn;

pub(crate) use chat::{
    chat_history_listing, chat_memory_listing, chat_session_summaries, chat_sessions_listing,
    fresh_chat_session_id, load_chat_session_state,
};
pub use command::run;
pub(crate) use command::{resume_chat, run_gateway_message, run_scheduled_job};
pub(crate) use mcp::{McpServerSpec, load_mcp_servers_from_workspace};
pub(crate) use options::{AgentRunOptions, CliPermissionMode};
pub(crate) use permissions::CliPermissionPolicy;
pub(crate) use session_state::persist_turn_outcome;
pub(crate) use turn::{UiBridge, build_agent_harness_with};

use chat::run_agent_chat;
use delegation::{
    CHILD_DEFAULT_MAX_ITERATIONS, CliSubAgentSpawner, DEFAULT_MAX_CONCURRENT_CHILDREN,
};
use events::{StdoutJsonEventSink, StdoutTokenSink};
use mcp::{agent_mcp, build_mcp_registry_for_server, parse_mcp_server};
use memory::{CliMemoryCandidateSink, CliProjectMemoryProvider};
use options::{
    AgentSessionMode, AgentToolSet, GatewayApproval, parse_agent_chat_options,
    parse_agent_resume_options, parse_agent_run_options, resolve_configured_fallback_routes,
};
use permissions::GatewayApprovalPolicy;
use plugins::load_plugins;
use session_state::{
    latest_session_id, persist_session_records, prepare_session_state, replay_session_messages,
};
use skills::{CliSkillProposalSink, CliSkillProvider};
use tooling::build_tool_registry;
use turn::{build_agent_harness, run_agent_turn};

#[cfg(test)]
mod tests {
    use super::*;

    use codel00p_protocol::ProjectRef;
    use codel00p_session::SessionStore;
    use httpmock::{Method::POST, MockServer};
    use serde_json::json;

    fn test_config(dir: &std::path::Path) -> CliConfig {
        CliConfig {
            memory_db: dir.join("memory.sqlite"),
            organization_id: "test-org".to_string(),
            project: ProjectRef::new("test-project", "Test Project"),
        }
    }

    // A child agent run goes through the real provider transport, so mock one
    // chat-completions response and confirm the spawner runs a child, returns
    // its summary, and records the child session linked to its parent.
    #[test]
    fn cli_spawner_runs_a_child_and_records_lineage() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "child summary" },
                    "finish_reason": "stop"
                }]
            }));
        });

        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let parent_session_id = SessionId::from_static("parent-session");
        // SAFETY: guarded by LOCK so no other test mutates this var concurrently.
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token");
        }

        let spawner = CliSubAgentSpawner {
            config: config.clone(),
            parent_session_id: parent_session_id.clone(),
            workspace: dir.path().to_path_buf(),
            provider_registry: default_registry(),
            provider: "custom".to_string(),
            model: "test-model".to_string(),
            base_url: Some(server.base_url()),
            policy_preset: None,
            max_iterations: 2,
            concurrency: Arc::new(tokio::sync::Semaphore::new(2)),
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let outcome = runtime
            .block_on(spawner.spawn(DelegatedTask::new("summarize the project")))
            .expect("spawn child");

        // SAFETY: still under LOCK.
        unsafe {
            std::env::remove_var("CODEL00P_PROVIDER_CUSTOM_API_KEY");
        }

        mock.assert();
        assert_eq!(outcome.summary(), "child summary");
        assert_eq!(outcome.tool_calls(), 0);

        // The child session is persisted with the parent as its lineage.
        let store = open_session_store(&config).expect("session store");
        let metadata = store
            .metadata(outcome.child_session_id())
            .expect("child session persisted");
        assert_eq!(metadata.parent_session_id(), Some(&parent_session_id));
    }
}

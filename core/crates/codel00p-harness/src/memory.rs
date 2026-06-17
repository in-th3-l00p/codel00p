use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_memory::{
    ExplicitMemoryExtractor, MemoryCandidateExtractor, MemoryCandidateInput, MemoryExtractionInput,
    MemoryQuery, MemoryRepository,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryVisibility, ProjectRef};
use serde::{Deserialize, Serialize};

use crate::{
    errors::HarnessError,
    session::{SessionId, TurnId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMemoryRequest {
    session_id: SessionId,
    turn_id: TurnId,
    message_count: usize,
}

impl ProjectMemoryRequest {
    pub fn new(session_id: SessionId, turn_id: TurnId, message_count: usize) -> Self {
        Self {
            session_id,
            turn_id,
            message_count,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnMemoryExtractionRequest {
    session_id: SessionId,
    turn_id: TurnId,
    assistant_message: Option<String>,
    message_count: usize,
}

impl TurnMemoryExtractionRequest {
    pub fn new(
        session_id: SessionId,
        turn_id: TurnId,
        assistant_message: Option<String>,
        message_count: usize,
    ) -> Self {
        Self {
            session_id,
            turn_id,
            assistant_message,
            message_count,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn assistant_message(&self) -> Option<&str> {
        self.assistant_message.as_deref()
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }
}

#[async_trait]
pub trait TurnMemoryExtractor: Send + Sync {
    async fn extract(
        &self,
        request: TurnMemoryExtractionRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError>;
}

pub struct ExplicitTurnMemoryExtractor {
    project: ProjectRef,
    tags: Vec<String>,
    extractor: ExplicitMemoryExtractor,
}

impl ExplicitTurnMemoryExtractor {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            tags: Vec::new(),
            extractor: ExplicitMemoryExtractor,
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        if let Some(tag) = non_empty_filter(tag.into()) {
            self.tags.push(tag);
        }
        self
    }
}

#[async_trait]
impl TurnMemoryExtractor for ExplicitTurnMemoryExtractor {
    async fn extract(
        &self,
        request: TurnMemoryExtractionRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError> {
        let Some(assistant_message) = request.assistant_message() else {
            return Ok(Vec::new());
        };
        let mut input = MemoryExtractionInput::new(
            self.project.clone(),
            MemorySource::turn(request.session_id().clone(), request.turn_id().clone()),
            assistant_message,
        );
        for tag in &self.tags {
            input = input.with_tag(tag);
        }

        self.extractor
            .extract(input)
            .map_err(|error| HarnessError::InferenceFailed {
                message: format!("memory extraction failed: {error}"),
            })
    }
}

// ===========================================================================
// Post-session memory recommendations (Memory 2.0): after a productive turn,
// automatically *recommend* memory candidates into the same review queue that
// explicit `remember:` directives flow into — never auto-approved. This mirrors
// the capability auto-extractor: a deterministic recommender is the must-have,
// with an optional LLM-assisted variant gated behind integration env vars.
// ===========================================================================

/// One executed tool call as a [`MemoryRecommender`] sees it: the tool name and
/// an optional short result snippet. (We keep this lighter than the capability
/// extractor's call record — a recommendation only needs to know *what* the
/// agent did, not replay it.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecommendationToolCall {
    name: String,
    result_snippet: Option<String>,
}

impl RecommendationToolCall {
    pub fn new(name: impl Into<String>, result_snippet: Option<String>) -> Self {
        Self {
            name: name.into(),
            result_snippet,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn result_snippet(&self) -> Option<&str> {
        self.result_snippet.as_deref()
    }
}

/// What a [`MemoryRecommender`] inspects at turn end to decide whether the work
/// produced a durable, memorable fact worth proposing for review. Modeled on
/// `CapabilityExtractionRequest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnMemoryRecommendationRequest {
    session_id: SessionId,
    turn_id: TurnId,
    goal: String,
    assistant_message: Option<String>,
    tool_calls: Vec<RecommendationToolCall>,
}

impl TurnMemoryRecommendationRequest {
    pub fn new(
        session_id: SessionId,
        turn_id: TurnId,
        goal: String,
        assistant_message: Option<String>,
        tool_calls: Vec<(String, Option<String>)>,
    ) -> Self {
        Self {
            session_id,
            turn_id,
            goal,
            assistant_message,
            tool_calls: tool_calls
                .into_iter()
                .map(|(name, snippet)| RecommendationToolCall::new(name, snippet))
                .collect(),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn goal(&self) -> &str {
        &self.goal
    }

    pub fn assistant_message(&self) -> Option<&str> {
        self.assistant_message.as_deref()
    }

    pub fn tool_calls(&self) -> &[RecommendationToolCall] {
        &self.tool_calls
    }
}

/// Recommends 0..N memory candidates from a completed turn. Returning candidates
/// queues them for human review — it never approves anything. A no-op (empty
/// `Vec`) is the right answer for read-only or trivial turns.
#[async_trait]
pub trait MemoryRecommender: Send + Sync {
    async fn recommend(
        &self,
        request: TurnMemoryRecommendationRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError>;
}

/// Tool names that mutate the workspace (or commit it). A turn is "productive"
/// when it ran at least `min_mutations` of these *and* produced a final answer.
const MUTATING_TOOLS: &[&str] = &[
    "create_file",
    "update_file",
    "delete_file",
    "apply_patch",
    "git_commit",
];

fn is_mutating_tool(name: &str) -> bool {
    MUTATING_TOOLS.contains(&name)
}

/// Deterministic, fully offline recommender. After a *productive* turn — at
/// least `min_mutations` workspace-mutating tool calls plus a final assistant
/// answer — it emits a single candidate summarizing what was done: the goal plus
/// the distinct mutating tools used, tagged `auto-recommended`, sourced to the
/// turn. Read-only or unfinished turns recommend nothing.
///
/// IDs follow the explicit extractor's `memory-candidate-{session}-{turn}-{i}`
/// scheme so the queue is consistent regardless of how a candidate arrived.
#[derive(Clone, Debug)]
pub struct DeterministicMemoryRecommender {
    project: ProjectRef,
    tags: Vec<String>,
    min_mutations: usize,
    kind: MemoryKind,
}

impl DeterministicMemoryRecommender {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            tags: vec!["auto-recommended".to_string()],
            min_mutations: 1,
            kind: MemoryKind::Workflow,
        }
    }

    /// Require at least `min` mutating tool calls before recommending. A value of
    /// `0` is treated as `1` (a recommendation always needs some mutating work).
    pub fn with_min_mutations(mut self, min: usize) -> Self {
        self.min_mutations = min.max(1);
        self
    }

    /// Override the kind assigned to recommended candidates (default
    /// [`MemoryKind::Workflow`]).
    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = kind;
        self
    }

    /// Add an extra tag (besides the default `auto-recommended`).
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        if let Some(tag) = non_empty_filter(tag.into()) {
            self.tags.push(tag);
        }
        self
    }
}

#[async_trait]
impl MemoryRecommender for DeterministicMemoryRecommender {
    async fn recommend(
        &self,
        request: TurnMemoryRecommendationRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError> {
        // A turn is only memorable once it has actually finished with an answer.
        let Some(answer) = request
            .assistant_message()
            .map(str::trim)
            .filter(|message| !message.is_empty())
        else {
            return Ok(Vec::new());
        };

        // Collect the distinct mutating tools used, preserving first-seen order.
        let mut mutating: Vec<&str> = Vec::new();
        let mut mutation_count = 0usize;
        for call in request.tool_calls() {
            if is_mutating_tool(call.name()) {
                mutation_count += 1;
                if !mutating.contains(&call.name()) {
                    mutating.push(call.name());
                }
            }
        }
        if mutation_count < self.min_mutations {
            return Ok(Vec::new());
        }

        let goal = first_line(request.goal());
        let goal = if goal.is_empty() {
            first_line(answer)
        } else {
            goal
        };
        if goal.is_empty() {
            return Ok(Vec::new());
        }

        let content = format!(
            "Workflow: \"{goal}\" was accomplished using {tools}.",
            tools = mutating.join(", "),
        );

        let id = format!(
            "memory-candidate-{}-{}-1",
            request.session_id().as_str(),
            request.turn_id().as_str(),
        );
        let mut candidate = MemoryCandidateInput::new(
            id,
            self.project.clone(),
            self.kind,
            content,
            MemorySource::turn(request.session_id().clone(), request.turn_id().clone()),
        );
        for tag in &self.tags {
            candidate = candidate.with_tag(tag);
        }

        Ok(vec![candidate])
    }
}

/// First non-empty line of `text`, trimmed and capped, for deterministic
/// single-line summaries.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.chars().count() > 120 {
        format!("{}…", line.chars().take(119).collect::<String>())
    } else {
        line.to_string()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryCandidateSinkOutcome {
    created_ids: Vec<String>,
    duplicate_ids: Vec<String>,
}

impl MemoryCandidateSinkOutcome {
    pub fn from_parts(created_ids: Vec<String>, duplicate_ids: Vec<String>) -> Self {
        Self {
            created_ids,
            duplicate_ids,
        }
    }

    pub fn created_ids(&self) -> &[String] {
        &self.created_ids
    }

    pub fn duplicate_ids(&self) -> &[String] {
        &self.duplicate_ids
    }

    pub fn total_seen(&self) -> usize {
        self.created_ids.len() + self.duplicate_ids.len()
    }
}

#[async_trait]
pub trait MemoryCandidateSink: Send + Sync {
    async fn persist(
        &self,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<MemoryCandidateSinkOutcome, HarnessError>;
}

pub struct MemoryRepositoryCandidateSink<R> {
    repository: Arc<Mutex<R>>,
}

impl<R> MemoryRepositoryCandidateSink<R> {
    pub fn new(repository: R) -> Self {
        Self {
            repository: Arc::new(Mutex::new(repository)),
        }
    }

    pub fn new_shared(repository: Arc<Mutex<R>>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R> MemoryCandidateSink for MemoryRepositoryCandidateSink<R>
where
    R: MemoryRepository + Send,
{
    async fn persist(
        &self,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<MemoryCandidateSinkOutcome, HarnessError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| HarnessError::Configuration {
                message: "memory candidate repository lock was poisoned".to_string(),
            })?;
        let mut outcome = MemoryCandidateSinkOutcome::default();

        for candidate in candidates {
            let id = candidate.id().to_string();
            match repository.create_candidate(candidate) {
                Ok(_) => outcome.created_ids.push(id),
                Err(
                    codel00p_memory::MemoryError::MemoryAlreadyExists { .. }
                    | codel00p_memory::MemoryError::DuplicateMemory { .. },
                ) => outcome.duplicate_ids.push(id),
                Err(error) => {
                    return Err(HarnessError::InferenceFailed {
                        message: format!("memory candidate persistence failed: {error}"),
                    });
                }
            }
        }

        Ok(outcome)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMemoryContext {
    items: Vec<ProjectMemoryItem>,
}

impl ProjectMemoryContext {
    pub fn new(items: Vec<ProjectMemoryItem>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[ProjectMemoryItem] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryPromptAssembler;

impl MemoryPromptAssembler {
    pub fn assemble(&self, memory: &ProjectMemoryContext) -> Option<String> {
        if memory.is_empty() {
            return None;
        }

        let mut items = memory.items().to_vec();
        items.sort_by(|left, right| left.id().cmp(right.id()));

        let mut prompt = String::from("Project memory:");
        for item in items {
            prompt.push_str(&format!(
                "\n- id: {}\n  kind: {}\n  tags: {}\n  reason: {}\n  content: {}",
                item.id(),
                memory_kind_label(item.kind()),
                format_prompt_field(&item.tags().join(",")),
                format_prompt_field(item.reason()),
                format_prompt_field(item.content())
            ));
        }

        Some(prompt)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMemoryItem {
    id: String,
    kind: MemoryKind,
    content: String,
    tags: Vec<String>,
    reason: String,
}

impl ProjectMemoryItem {
    pub fn new(
        id: impl Into<String>,
        kind: MemoryKind,
        content: impl Into<String>,
        tags: Vec<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            content: content.into(),
            tags,
            reason: reason.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(&self) -> MemoryKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[async_trait]
pub trait ProjectMemoryProvider: Send + Sync {
    async fn retrieve(
        &self,
        request: ProjectMemoryRequest,
    ) -> Result<ProjectMemoryContext, HarnessError>;
}

pub struct MemoryRepositoryProjectMemoryProvider<R> {
    project: ProjectRef,
    repository: R,
    kind: Option<MemoryKind>,
    tag: Option<String>,
    text: Option<String>,
    visibility: Option<MemoryVisibility>,
    limit: Option<usize>,
}

impl<R> MemoryRepositoryProjectMemoryProvider<R> {
    pub fn new(project: ProjectRef, repository: R) -> Self {
        Self {
            project,
            repository,
            kind: None,
            tag: None,
            text: None,
            visibility: None,
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Caps retrieved project memory to the given maximum visibility scope
    /// (narrow→wide). Unset returns every visibility, matching prior behavior.
    pub fn with_visibility(mut self, visibility: MemoryVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = non_empty_filter(tag.into());
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = non_empty_filter(text.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

#[async_trait]
impl<R> ProjectMemoryProvider for MemoryRepositoryProjectMemoryProvider<R>
where
    R: MemoryRepository + Send + Sync,
{
    async fn retrieve(
        &self,
        _request: ProjectMemoryRequest,
    ) -> Result<ProjectMemoryContext, HarnessError> {
        let mut query = MemoryQuery::new(self.project.clone());
        if let Some(kind) = self.kind {
            query = query.with_kind(kind);
        }
        if let Some(tag) = &self.tag {
            query = query.with_tag(tag);
        }
        if let Some(text) = &self.text {
            query = query.with_text(text);
        }
        if let Some(visibility) = self.visibility {
            query = query.with_visibility(visibility);
        }
        if let Some(limit) = self.limit {
            query = query.with_limit(limit);
        }

        let items = self
            .repository
            .retrieve(query)
            .map_err(|error| HarnessError::InferenceFailed {
                message: format!("project memory retrieval failed: {error}"),
            })?
            .into_iter()
            .map(|memory| {
                ProjectMemoryItem::new(
                    memory.entry().id(),
                    memory.entry().kind(),
                    memory.entry().content(),
                    memory.entry().tags().to_vec(),
                    memory.reason(),
                )
            })
            .collect();

        Ok(ProjectMemoryContext::new(items))
    }
}

fn non_empty_filter(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn memory_kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn format_prompt_field(value: &str) -> String {
    value.lines().collect::<Vec<_>>().join("\n    ")
}

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_memory::{
    ExplicitMemoryExtractor, MemoryCandidateExtractor, MemoryCandidateInput, MemoryExtractionInput,
    MemoryQuery, MemoryRepository,
};
use codel00p_protocol::{MemoryKind, MemorySource, ProjectRef};
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
                Err(codel00p_memory::MemoryError::MemoryAlreadyExists { .. }) => {
                    outcome.duplicate_ids.push(id);
                }
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
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
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

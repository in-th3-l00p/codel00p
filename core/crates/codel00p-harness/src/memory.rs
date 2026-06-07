use async_trait::async_trait;
use codel00p_memory::{MemoryQuery, MemoryRepository};
use codel00p_protocol::{MemoryKind, ProjectRef};
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
    limit: Option<usize>,
}

impl<R> MemoryRepositoryProjectMemoryProvider<R> {
    pub fn new(project: ProjectRef, repository: R) -> Self {
        Self {
            project,
            repository,
            kind: None,
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
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

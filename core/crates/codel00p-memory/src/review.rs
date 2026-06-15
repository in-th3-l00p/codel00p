//! Review decisions, edits, and audit log events for memory lifecycle changes.

use codel00p_protocol::MemoryStatus;
use codel00p_storage::AppendLogEntry;
use serde::{Deserialize, Serialize};

use crate::{MemoryError, util::non_empty_filter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewDecision {
    Approve { actor: String },
    Reject { actor: String, reason: String },
    Archive { actor: String, reason: String },
}

impl ReviewDecision {
    pub fn approve(actor: impl Into<String>) -> Self {
        Self::Approve {
            actor: actor.into(),
        }
    }

    pub fn reject(actor: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Reject {
            actor: actor.into(),
            reason: reason.into(),
        }
    }

    pub fn archive(actor: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Archive {
            actor: actor.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn actor(&self) -> &str {
        match self {
            Self::Approve { actor } | Self::Reject { actor, .. } | Self::Archive { actor, .. } => {
                actor
            }
        }
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        match self {
            Self::Approve { .. } => None,
            Self::Reject { reason, .. } | Self::Archive { reason, .. } => Some(reason),
        }
    }

    pub(crate) fn action(&self) -> MemoryAuditAction {
        match self {
            Self::Approve { .. } => MemoryAuditAction::Approved,
            Self::Reject { .. } => MemoryAuditAction::Rejected,
            Self::Archive { .. } => MemoryAuditAction::Archived,
        }
    }

    pub(crate) fn next_status(&self) -> MemoryStatus {
        match self {
            Self::Approve { .. } => MemoryStatus::Approved,
            Self::Reject { .. } => MemoryStatus::Rejected,
            Self::Archive { .. } => MemoryStatus::Archived,
        }
    }
}

/// A request to fold a duplicate memory (the source) into a canonical one (the
/// target). Mirrors [`ReviewDecision`]/[`MemoryEdit`]: the operation lives on the
/// repository; this only carries who did it and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryMerge {
    actor: String,
    reason: Option<String>,
}

impl MemoryMerge {
    pub fn new(actor: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = non_empty_filter(reason.into());
        self
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEdit {
    actor: String,
    content: String,
    reason: Option<String>,
}

impl MemoryEdit {
    pub fn replace_content(actor: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            content: content.into(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = non_empty_filter(reason.into());
        self
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub(crate) fn validate(&self) -> Result<(), MemoryError> {
        if self.content.trim().is_empty() {
            return Err(MemoryError::InvalidEdit {
                message: "memory content cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAuditAction {
    CandidateCreated,
    Approved,
    Rejected,
    Archived,
    Edited,
    Merged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAuditEvent {
    pub(crate) memory_id: String,
    sequence: u64,
    action: MemoryAuditAction,
    actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_content: Option<String>,
    /// On a merge, the target this memory was folded into. Present only on the
    /// source side of a merge; the target's event leaves it `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    merged_into: Option<String>,
}

impl MemoryAuditEvent {
    pub(crate) fn candidate_created(memory_id: impl Into<String>) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::CandidateCreated,
            actor: "system".to_string(),
            reason: None,
            previous_content: None,
            new_content: None,
            merged_into: None,
        }
    }

    pub(crate) fn reviewed(memory_id: impl Into<String>, decision: &ReviewDecision) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: decision.action(),
            actor: decision.actor().to_string(),
            reason: decision.reason().map(ToString::to_string),
            previous_content: None,
            new_content: None,
            merged_into: None,
        }
    }

    /// The audit event written to the **source** memory of a merge: it records
    /// the target it was folded into and is paired with an [`Self::merged_from`]
    /// event on that target.
    pub(crate) fn merged(
        memory_id: impl Into<String>,
        merge: &MemoryMerge,
        merged_into: impl Into<String>,
    ) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::Merged,
            actor: merge.actor().to_string(),
            reason: merge.reason().map(ToString::to_string),
            previous_content: None,
            new_content: None,
            merged_into: Some(merged_into.into()),
        }
    }

    /// The audit event written to the **target** memory of a merge: it names the
    /// absorbed source in its reason and leaves `merged_into` unset, marking it
    /// as the receiving side.
    pub(crate) fn merged_from(
        memory_id: impl Into<String>,
        merge: &MemoryMerge,
        source_id: &str,
    ) -> Self {
        let reason = match merge.reason() {
            Some(reason) => format!("absorbed {source_id}: {reason}"),
            None => format!("absorbed {source_id}"),
        };
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::Merged,
            actor: merge.actor().to_string(),
            reason: Some(reason),
            previous_content: None,
            new_content: None,
            merged_into: None,
        }
    }

    pub(crate) fn edited(
        memory_id: impl Into<String>,
        edit: &MemoryEdit,
        previous_content: impl Into<String>,
        new_content: impl Into<String>,
    ) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::Edited,
            actor: edit.actor().to_string(),
            reason: edit.reason().map(ToString::to_string),
            previous_content: Some(previous_content.into()),
            new_content: Some(new_content.into()),
            merged_into: None,
        }
    }

    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn action(&self) -> MemoryAuditAction {
        self.action
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn previous_content(&self) -> Option<&str> {
        self.previous_content.as_deref()
    }

    pub fn new_content(&self) -> Option<&str> {
        self.new_content.as_deref()
    }

    /// On a merge's source event, the target memory it was folded into.
    pub fn merged_into(&self) -> Option<&str> {
        self.merged_into.as_deref()
    }

    pub(crate) fn from_log_entry(entry: AppendLogEntry) -> Result<Self, MemoryError> {
        let mut event: Self = serde_json::from_value(entry.payload().clone())?;
        event.sequence = entry.sequence();
        Ok(event)
    }
}

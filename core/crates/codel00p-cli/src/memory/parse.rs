use codel00p_protocol::{
    EvidenceKind, MemoryKind, MemorySensitivity, MemoryStatus, MemoryVisibility,
};

use crate::config::CliResult;

pub(super) fn parse_status(value: &str) -> CliResult<MemoryStatus> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

pub(super) fn parse_kind(value: &str) -> CliResult<MemoryKind> {
    match value {
        "architecture" => Ok(MemoryKind::Architecture),
        "convention" => Ok(MemoryKind::Convention),
        "workflow" => Ok(MemoryKind::Workflow),
        "decision" => Ok(MemoryKind::Decision),
        "deployment" => Ok(MemoryKind::Deployment),
        "troubleshooting" => Ok(MemoryKind::Troubleshooting),
        _ => Err(format!("unknown memory kind: {value}")),
    }
}

pub(super) fn parse_sensitivity(value: &str) -> CliResult<MemorySensitivity> {
    match value {
        "normal" => Ok(MemorySensitivity::Normal),
        "sensitive" => Ok(MemorySensitivity::Sensitive),
        _ => Err(format!("unknown memory sensitivity: {value}")),
    }
}

pub(super) fn parse_visibility(value: &str) -> CliResult<MemoryVisibility> {
    match value {
        "private" => Ok(MemoryVisibility::Private),
        "project" => Ok(MemoryVisibility::Project),
        "team" => Ok(MemoryVisibility::Team),
        "org" => Ok(MemoryVisibility::Org),
        _ => Err(format!("unknown memory visibility: {value}")),
    }
}

pub(super) fn parse_evidence_kind(value: &str) -> CliResult<EvidenceKind> {
    match value {
        "file" => Ok(EvidenceKind::File),
        "url" => Ok(EvidenceKind::Url),
        "pr" => Ok(EvidenceKind::Pr),
        "issue" => Ok(EvidenceKind::Issue),
        "commit" => Ok(EvidenceKind::Commit),
        "other" => Ok(EvidenceKind::Other),
        _ => Err(format!("unknown evidence kind: {value}")),
    }
}

pub(super) fn evidence_kind_label(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::File => "file",
        EvidenceKind::Url => "url",
        EvidenceKind::Pr => "pr",
        EvidenceKind::Issue => "issue",
        EvidenceKind::Commit => "commit",
        EvidenceKind::Other => "other",
    }
}

pub(super) fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
    }
}

pub(super) fn sensitivity_label(sensitivity: MemorySensitivity) -> &'static str {
    match sensitivity {
        MemorySensitivity::Normal => "normal",
        MemorySensitivity::Sensitive => "sensitive",
    }
}

pub(super) fn visibility_label(visibility: MemoryVisibility) -> &'static str {
    match visibility {
        MemoryVisibility::Private => "private",
        MemoryVisibility::Project => "project",
        MemoryVisibility::Team => "team",
        MemoryVisibility::Org => "org",
    }
}

pub(super) fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

pub(super) fn audit_action_label(action: codel00p_memory::MemoryAuditAction) -> &'static str {
    match action {
        codel00p_memory::MemoryAuditAction::CandidateCreated => "candidate_created",
        codel00p_memory::MemoryAuditAction::Approved => "approved",
        codel00p_memory::MemoryAuditAction::Rejected => "rejected",
        codel00p_memory::MemoryAuditAction::Archived => "archived",
        codel00p_memory::MemoryAuditAction::Edited => "edited",
        codel00p_memory::MemoryAuditAction::Merged => "merged",
        codel00p_memory::MemoryAuditAction::Split => "split",
        codel00p_memory::MemoryAuditAction::EvidenceAdded => "evidence_added",
    }
}

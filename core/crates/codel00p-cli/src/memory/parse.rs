use codel00p_protocol::{MemoryKind, MemorySensitivity, MemoryStatus};

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
    }
}

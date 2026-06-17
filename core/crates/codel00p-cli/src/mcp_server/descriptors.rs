//! Static MCP tool and resource-template descriptors.

use super::*;

pub(super) fn mcp_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "memory_similar",
            "description": "Score active near-duplicate codel00p project memory.",
            "inputSchema": {
                "type": "object",
                "required": ["content", "kind"],
                "properties": {
                    "content": { "type": "string" },
                    "kind": { "type": "string" },
                    "threshold": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_stale",
            "description": "Find approved codel00p project memory likely superseded by newer active memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string" },
                    "threshold": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_quality",
            "description": "List active codel00p project memory with low advisory quality scores.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string" },
                    "kind": { "type": "string" },
                    "sensitivity": { "type": "string" },
                    "tag": { "type": "string" },
                    "max_score": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_search",
            "description": "Search approved codel00p project memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "kind": { "type": "string" },
                    "sensitivity": { "type": "string" },
                    "tag": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_list",
            "description": "List codel00p project memory records.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string" },
                    "kind": { "type": "string" },
                    "sensitivity": { "type": "string" },
                    "tag": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_show",
            "description": "Show one codel00p memory record by id.",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_audit",
            "description": "Show audit history for one codel00p memory record.",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_create_candidate",
            "description": "Create candidate codel00p project memory for review.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "kind", "content", "session_id", "turn_id"],
                "properties": {
                    "id": { "type": "string" },
                    "kind": { "type": "string" },
                    "content": { "type": "string" },
                    "session_id": { "type": "string" },
                    "turn_id": { "type": "string" },
                    "source_uri": { "type": "string" },
                    "sensitivity": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }
        }),
        json!({
            "name": "memory_approve",
            "description": "Approve one codel00p project memory candidate.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_reject",
            "description": "Reject one codel00p project memory candidate.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_archive",
            "description": "Archive one codel00p project memory record.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_edit",
            "description": "Edit one codel00p project memory record.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "content"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "content": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_restore",
            "description": "Restore one codel00p project memory record from an edit audit sequence.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "sequence", "actor"],
                "properties": {
                    "id": { "type": "string" },
                    "sequence": { "type": "integer", "minimum": 1 },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_merge",
            "description": "Merge a duplicate codel00p memory (source) into a canonical one (target), archiving the source.",
            "inputSchema": {
                "type": "object",
                "required": ["source_id", "target_id", "actor"],
                "properties": {
                    "source_id": { "type": "string" },
                    "target_id": { "type": "string" },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_split",
            "description": "Split one codel00p memory into two: the source stays active, a new candidate is created carrying part of the content.",
            "inputSchema": {
                "type": "object",
                "required": ["source_id", "new_id", "actor", "content"],
                "properties": {
                    "source_id": { "type": "string" },
                    "new_id": { "type": "string" },
                    "actor": { "type": "string" },
                    "content": { "type": "string" },
                    "source_content": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "session_show",
            "description": "Replay one codel00p agent session by id.",
            "inputSchema": {
                "type": "object",
                "required": ["session_id"],
                "properties": {
                    "session_id": { "type": "string" }
                }
            }
        }),
    ]
}

pub(super) fn mcp_resource_templates() -> Vec<Value> {
    vec![
        json!({
            "uriTemplate": "codel00p://memory/{id}",
            "name": "codel00p memory record",
            "description": "Read one codel00p project memory record as JSON.",
            "mimeType": "application/json"
        }),
        json!({
            "uriTemplate": "codel00p://sessions/{session_id}",
            "name": "codel00p session replay",
            "description": "Read one codel00p agent session replay as JSON.",
            "mimeType": "application/json"
        }),
    ]
}

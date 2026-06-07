export type ProtocolVersion = "codel00p.protocol.v1";

export type SessionRole = "system" | "user" | "assistant" | "tool";

export type MemoryKind =
  | "architecture"
  | "workflow"
  | "decision"
  | "debugging"
  | "deployment";

export type MemoryStatus = "candidate" | "approved" | "rejected" | "archived";

export type SessionMessage = {
  role: SessionRole;
  content: string;
  tool_call_id?: string;
  tool_name?: string;
  payload?: unknown;
};

export type ProjectRef = {
  project_id: string;
  name: string;
  repository_url?: string;
};

export type MemoryEntry = {
  id: string;
  project: {
    id: string;
    name: string;
  };
  kind: MemoryKind;
  status: MemoryStatus;
  content: string;
  tags: string[];
  source?: {
    session_id?: string;
    turn_id?: string;
  };
};

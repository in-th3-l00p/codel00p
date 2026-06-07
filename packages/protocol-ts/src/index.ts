export type ProtocolVersion = "codel00p.protocol.v1";

export type SessionRole = "system" | "user" | "assistant" | "tool";

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

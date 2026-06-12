export type ProtocolVersion = "codel00p.protocol.v1";

export type SessionRole = "system" | "user" | "assistant" | "tool";

export type MemoryKind =
  | "architecture"
  | "convention"
  | "workflow"
  | "decision"
  | "deployment"
  | "troubleshooting";

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

// --- Cloud / team control-plane contracts ---
//
// Clerk owns identity and membership; these shapes key codel00p product data by
// Clerk organization and user identifiers.

export type OrgRole = "admin" | "member";

export type OrgRef = {
  id: string;
  name: string;
  slug?: string;
};

/** The authenticated caller, as returned by `GET /me`. */
export type Viewer = {
  user_id: string;
  email?: string;
  org?: OrgRef;
  org_role?: OrgRole;
};

/** A project owned by an organization. */
export type Project = {
  id: string;
  org_id: string;
  name: string;
  slug: string;
  repository_url?: string;
};

/** Request body for creating a project; the owning org comes from the session. */
export type NewProject = {
  name: string;
  repository_url?: string;
};

export type MemorySensitivity = "normal" | "sensitive";

/** Request to push a memory candidate into a project's review queue. */
export type NewMemoryCandidate = {
  kind: MemoryKind;
  content: string;
  tags?: string[];
  sensitivity?: MemorySensitivity;
  source_uri?: string;
};

export type MemoryReviewAction =
  | "created"
  | "approved"
  | "rejected"
  | "archived";

/** One entry in a memory review audit trail. */
export type MemoryAuditEntry = {
  memory_id: string;
  action: MemoryReviewAction;
  actor: string;
};

/** Partial update to a project. */
export type ProjectUpdate = {
  name?: string;
  repository_url?: string;
};

export type McpTransport = "stdio" | "http";

/** A team MCP server configuration (org-shared pool of external tools). */
export type McpServer = {
  id: string;
  org_id: string;
  project_id: string;
  name: string;
  transport: McpTransport;
  command?: string;
  url?: string;
  enabled: boolean;
  created_by: string;
};

export type NewMcpServer = {
  name: string;
  transport: McpTransport;
  command?: string;
  url?: string;
  enabled?: boolean;
};

export type McpServerUpdate = {
  name?: string;
  transport?: McpTransport;
  command?: string;
  url?: string;
  enabled?: boolean;
};

/** A stored agent definition (org-shared pool of agents a team can run). */
export type Agent = {
  id: string;
  org_id: string;
  project_id: string;
  name: string;
  description?: string;
  instructions?: string;
  provider: string;
  model: string;
  mcp_server_ids?: string[];
  created_by: string;
};

export type NewAgent = {
  name: string;
  description?: string;
  instructions?: string;
  provider: string;
  model: string;
  mcp_server_ids?: string[];
};

export type AgentUpdate = {
  name?: string;
  description?: string;
  instructions?: string;
  provider?: string;
  model?: string;
  mcp_server_ids?: string[];
};

export const providerPolicyPresets = [
  {
    id: "allow_all",
    display_name: "Allow All",
    description: "Allow any registered provider profile without additional policy constraints."
  },
  {
    id: "enterprise_direct",
    display_name: "Enterprise Direct",
    description: "Allow direct first-wave corporate provider profiles."
  },
  {
    id: "enterprise_cloud_proxy",
    display_name: "Enterprise Cloud Proxy",
    description: "Require direct provider routes to resolve through codel00p CloudProxy."
  },
  {
    id: "enterprise_custom_gateway",
    display_name: "Enterprise Custom Gateway",
    description: "Allow only the configured OpenAI-compatible gateway profile."
  },
  {
    id: "enterprise_managed_identity",
    display_name: "Enterprise Managed Identity",
    description: "Require direct provider credentials from managed identity sources."
  },
  {
    id: "enterprise_organization_credentials",
    display_name: "Enterprise Organization Credentials",
    description: "Require direct provider credentials from organization-managed sources."
  },
  {
    id: "enterprise_direct_agentic",
    display_name: "Enterprise Direct Agentic",
    description: "Allow direct providers and require agentic model capability flags in catalogs."
  }
] as const satisfies readonly {
  id: string;
  display_name: string;
  description: string;
}[];

export type ProviderPolicyPresetId = (typeof providerPolicyPresets)[number]["id"];

export type ProviderPolicyPreset = {
  id: ProviderPolicyPresetId;
  display_name: string;
  description: string;
};

export function isProviderPolicyPresetId(value: string): value is ProviderPolicyPresetId {
  return providerPolicyPresets.some((preset) => preset.id === value);
}

export function providerPolicyPresetById(id: string): ProviderPolicyPreset | undefined {
  return providerPolicyPresets.find((preset) => preset.id === id);
}

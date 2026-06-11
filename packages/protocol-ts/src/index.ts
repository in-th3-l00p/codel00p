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

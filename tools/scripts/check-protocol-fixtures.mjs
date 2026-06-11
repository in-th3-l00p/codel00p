import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const fixturesRoot = join(root, "core", "crates", "codel00p-protocol", "tests", "fixtures");
const contractPath = join(root, "packages", "protocol-ts", "contracts", "protocol.v1.json");
const contract = JSON.parse(readFileSync(contractPath, "utf8"));
const errors = [];

const readFixture = (name) =>
  JSON.parse(readFileSync(join(fixturesRoot, name), "utf8"));

for (const [index, message] of readFixture("session_messages.json").entries()) {
  if (!contract.sessionRoles.includes(message.role)) {
    errors.push(`session_messages[${index}] has unknown role ${message.role}`);
  }

  if (typeof message.content !== "string") {
    errors.push(`session_messages[${index}] content must be a string`);
  }

  if (message.role === "tool" && typeof message.tool_call_id !== "string") {
    errors.push(`session_messages[${index}] tool message must include tool_call_id`);
  }
}

const memoryEntry = readFixture("memory_entry.json");
if (!contract.memoryKinds.includes(memoryEntry.kind)) {
  errors.push(`memory_entry kind ${memoryEntry.kind} is not in protocol-ts contract`);
}
if (!contract.memoryStatuses.includes(memoryEntry.status)) {
  errors.push(`memory_entry status ${memoryEntry.status} is not in protocol-ts contract`);
}
for (const field of ["id", "content"]) {
  if (typeof memoryEntry[field] !== "string") {
    errors.push(`memory_entry ${field} must be a string`);
  }
}
if (typeof memoryEntry.project?.id !== "string" || typeof memoryEntry.project?.name !== "string") {
  errors.push("memory_entry project must contain string id and name");
}

const expectedProviderPolicyPresets = [
  {
    id: "allow_all",
    display_name: "Allow All",
    description: "Allow any registered provider profile without additional policy constraints.",
  },
  {
    id: "enterprise_direct",
    display_name: "Enterprise Direct",
    description: "Allow direct first-wave corporate provider profiles.",
  },
  {
    id: "enterprise_cloud_proxy",
    display_name: "Enterprise Cloud Proxy",
    description: "Require direct provider routes to resolve through codel00p CloudProxy.",
  },
  {
    id: "enterprise_custom_gateway",
    display_name: "Enterprise Custom Gateway",
    description: "Allow only the configured OpenAI-compatible gateway profile.",
  },
  {
    id: "enterprise_managed_identity",
    display_name: "Enterprise Managed Identity",
    description: "Require direct provider credentials from managed identity sources.",
  },
  {
    id: "enterprise_organization_credentials",
    display_name: "Enterprise Organization Credentials",
    description: "Require direct provider credentials from organization-managed sources.",
  },
  {
    id: "enterprise_direct_agentic",
    display_name: "Enterprise Direct Agentic",
    description: "Allow direct providers and require agentic model capability flags in catalogs.",
  },
];

if (JSON.stringify(contract.providerPolicyPresets) !== JSON.stringify(expectedProviderPolicyPresets)) {
  errors.push("providerPolicyPresets must match the built-in Rust provider policy preset metadata");
}

if (errors.length > 0) {
  console.error("Protocol fixture check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Protocol fixture check passed.");

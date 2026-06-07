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

if (errors.length > 0) {
  console.error("Protocol fixture check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Protocol fixture check passed.");

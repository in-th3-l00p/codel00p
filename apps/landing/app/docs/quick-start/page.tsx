import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Quick start — codel00p docs",
  description: "Run your first agent turn and review the memory it proposes.",
};

export default function QuickStartPage() {
  return (
    <>
      <DocHeader
        group="Getting started"
        title="Quick start"
        lead="Set a provider key, run a single agent turn, then review what it learned."
      />

      <H2>Run an agent turn</H2>
      <P>
        Every command is scoped to an organization and project and writes to a
        local memory database. Set those four global flags before the{" "}
        <Ic>agent</Ic> subcommand.
      </P>
      <Code>{`export ANTHROPIC_API_KEY=sk-ant-...

codel00p \\
  --memory-db ./codel00p.db \\
  --organization-id acme \\
  --project-id web \\
  --project-name "Web App" \\
  agent run "Summarize how this project is built and tested" \\
  --provider anthropic \\
  --model claude-opus-4-8 \\
  --tool-set all`}</Code>
      <P>
        <Ic>--tool-set</Ic> selects which tools the run may use:{" "}
        <Ic>read</Ic>, <Ic>edit</Ic>, <Ic>command</Ic>, <Ic>git</Ic>, or{" "}
        <Ic>all</Ic>. Add <Ic>--session-id</Ic> to persist the session so you can
        resume it later with <Ic>agent resume</Ic>.
      </P>

      <H2>Review the memory it proposed</H2>
      <P>
        After useful work the run proposes memory candidates. List them, then
        approve the ones worth keeping. The four scope flags are the same as
        above.
      </P>
      <Code>{`codel00p ...scope memory list
codel00p ...scope memory approve <memory-id>`}</Code>
      <P>
        Only approved memory reaches the model on future runs. From here, read{" "}
        <Ic>Architecture</Ic> to see how a turn actually executes.
      </P>

      <Note>
        <p>
          The scope flags before the subcommand are required on every command.
          The examples in the rest of these docs write them as{" "}
          <Ic>...scope</Ic> to stay readable.
        </p>
      </Note>
    </>
  );
}

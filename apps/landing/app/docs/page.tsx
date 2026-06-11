import type { Metadata } from "next";

import { DocHeader, H2, P, Ul, Ic, Strong } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Introduction — codel00p docs",
  description:
    "What codel00p is, how it is put together, and how to read these docs.",
};

export default function IntroductionPage() {
  return (
    <>
      <DocHeader
        title="Introduction"
        lead="codel00p is an open-source coding agent that turns finished work into reviewed, durable project memory. It runs locally from a single CLI."
      />

      <P>
        Most agents start every session from zero. They relearn the same
        repository structure, conventions, and decisions on every run. codel00p
        treats that knowledge as the product: a run does real repository work,
        proposes <Strong>memory candidates</Strong> from what it learned, and you
        review them. Only approved memory is fed back into future runs.
      </P>
      <P>
        Everything is stored in one local SQLite file you own. There is no
        service to sign up for and nothing leaves your machine unless you point a
        provider at a remote endpoint.
      </P>

      <H2>How it is built</H2>
      <P>
        codel00p is a small set of Rust crates with one clear responsibility
        each. The CLI is the first interface over them.
      </P>
      <Ul>
        <li>
          <Ic>codel00p-harness</Ic> — the agent turn loop, workspace-safe tools,
          permissions, and event stream.
        </li>
        <li>
          <Ic>codel00p-memory</Ic> — the candidate, approved, and archived
          lifecycle for project knowledge.
        </li>
        <li>
          <Ic>codel00p-providers</Ic> — one contract over Anthropic, OpenAI,
          Bedrock, Gemini, and OpenAI-compatible gateways.
        </li>
        <li>
          <Ic>codel00p-mcp</Ic> — Model Context Protocol client and server, so
          external tools and other agents can connect.
        </li>
        <li>
          <Ic>codel00p-storage</Ic>, <Ic>codel00p-session</Ic>, and{" "}
          <Ic>codel00p-protocol</Ic> — persistence, durable sessions, and the
          shared data contracts between crates.
        </li>
      </Ul>

      <H2>How to read these docs</H2>
      <P>
        Start with <Strong>Getting started</Strong> to install the CLI and run
        your first agent turn. Then read <Strong>Architecture</Strong> to
        understand how the pieces fit together. The <Strong>Guides</Strong> cover
        day-to-day tasks, and <Strong>Reference</Strong> lists every command.
      </P>
    </>
  );
}

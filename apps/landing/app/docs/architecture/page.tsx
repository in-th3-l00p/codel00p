import type { Metadata } from "next";

import {
  DocHeader,
  H2,
  P,
  Ol,
  Ul,
  Ic,
  Strong,
  Diagram,
} from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Architecture — codel00p docs",
  description:
    "How codel00p executes an agent turn and turns it into reviewed project memory.",
};

export default function ArchitectureOverviewPage() {
  return (
    <>
      <DocHeader
        group="Architecture"
        title="Overview"
        lead="codel00p is a few Rust crates with one job each. This chapter explains how a single agent turn flows through them, and how that turn becomes durable memory."
      />

      <P>
        At the center is the <Strong>harness</Strong>: it runs the turn loop,
        calls the selected provider, executes the tools the model asks for, and
        emits a typed event for every step. Around it sit the{" "}
        <Strong>memory</Strong> engine, the <Strong>provider</Strong> router, and
        the <Strong>storage</Strong> layer that makes sessions and memory
        durable.
      </P>

      <Diagram>{`   developer / CLI
         │  prompt + scope
         ▼
   ┌───────────────┐     reviewed memory     ┌──────────────┐
   │    harness    │ ◀────────────────────── │    memory    │
   │  (turn loop)  │ ──────────────────────▶ │  candidates  │
   └───────┬───────┘     new candidates      └──────────────┘
           │  ▲
   route   │  │ tool results
           ▼  │
   ┌───────────────┐   workspace + git    ┌──────────────┐
   │   providers   │   tools, MCP tools   │   storage    │
   │    (router)   │                      │   (sqlite)   │
   └───────────────┘                      └──────────────┘`}</Diagram>

      <H2>The life of a turn</H2>
      <P>A single <Ic>agent run</Ic> moves through these steps:</P>
      <Ol>
        <li>
          The CLI parses the scope and flags, opens the local database, and
          builds the harness with the chosen tool set and provider.
        </li>
        <li>
          The harness assembles context for the turn: the prompt, prior session
          messages, workspace metadata, and any <Strong>approved memory</Strong>{" "}
          relevant to the task.
        </li>
        <li>
          It calls the provider through a normalized model client and receives
          assistant text, tool calls, or both.
        </li>
        <li>
          Requested tools run against the workspace under the active permission
          mode. Results are appended to the session and emitted as events.
        </li>
        <li>
          The loop repeats until the model returns a final answer or hits the
          iteration limit, producing a structured outcome.
        </li>
        <li>
          At safe boundaries the harness can extract{" "}
          <Strong>memory candidates</Strong> from what happened, ready for
          review.
        </li>
      </Ol>

      <H2>Why it is split this way</H2>
      <P>
        Each crate owns a single product concern so the CLI, a future desktop
        app, and a future cloud platform can share the same behavior without
        forking it.
      </P>
      <Ul>
        <li>
          <Ic>codel00p-protocol</Ic> holds the data-only contracts every crate
          exchanges: session and turn ids, messages, events, tool calls, and
          memory entries.
        </li>
        <li>
          <Ic>codel00p-storage</Ic> is the only crate that talks to a backend.
          Everything else stores data through backend-neutral traits.
        </li>
        <li>
          The harness, memory, and provider crates depend on the contracts, not
          on each other&apos;s internals.
        </li>
      </Ul>
      <P>
        The next pages walk through each piece: the harness turn loop, the
        memory lifecycle, provider routing, and the storage and session layer.
      </P>
    </>
  );
}

import type { Metadata } from "next";

import { DocHeader, H2, P, Ul, Ol, Ic, Strong } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "The harness — codel00p docs",
  description:
    "The agent turn loop: context, tools, permissions, events, and lifecycle hooks.",
};

export default function HarnessPage() {
  return (
    <>
      <DocHeader
        group="Architecture"
        title="The harness"
        lead="codel00p-harness is the runtime. It owns the turn loop, tool execution, the workspace boundary, permissions, and the event stream."
      />

      <H2>The turn loop</H2>
      <P>A turn is a bounded loop, not an open-ended process:</P>
      <Ol>
        <li>accept a user turn;</li>
        <li>build context from session state and workspace metadata;</li>
        <li>call the provider through the model client;</li>
        <li>execute the tools the model requested;</li>
        <li>append tool results to the session;</li>
        <li>
          continue until a final assistant answer or the iteration limit set by{" "}
          <Ic>--max-iterations</Ic>;
        </li>
        <li>emit a typed event for every important step.</li>
      </Ol>
      <P>
        The result is a structured outcome the CLI, and later the desktop and
        cloud surfaces, can render or replay.
      </P>

      <H2>Tools and the workspace boundary</H2>
      <P>
        Tools are grouped into sets you opt into with <Ic>--tool-set</Ic>:{" "}
        <Ic>read</Ic> for inspection, <Ic>edit</Ic> for file changes,{" "}
        <Ic>command</Ic> for running programs, and <Ic>git</Ic> for repository
        state. Every tool is confined to the workspace root, so a run cannot read
        or write outside the project directory.
      </P>
      <P>
        Tools marked <Strong>concurrency-safe</Strong> can run together in one
        batch. Unsafe or unknown tools always run serially and split adjacent
        safe batches, and the harness preserves the model&apos;s original
        tool-call order when recording results and events.
      </P>

      <H2>Permissions</H2>
      <P>
        Tool execution runs under a permission mode set with{" "}
        <Ic>--permission-mode</Ic>:
      </P>
      <Ul>
        <li>
          <Ic>allow</Ic> — run permitted tools without prompting;
        </li>
        <li>
          <Ic>ask</Ic> — confirm before sensitive actions, with the option to
          remember a decision;
        </li>
        <li>
          <Ic>deny</Ic> — block the tool surface entirely.
        </li>
      </Ul>
      <P>
        The same checks cover external MCP tools, so connectors are governed the
        same way native tools are.
      </P>

      <H2>Events and lifecycle hooks</H2>
      <P>
        Every step emits a typed event. With <Ic>--stream-events</Ic> you see
        them live; with <Ic>--json-events</Ic> you get them as serialized
        records after the answer. The same events drive session replay.
      </P>
      <P>
        The loop also exposes lifecycle hooks so memory, compaction, and
        approvals can extend a turn without forking it: queue recall before a
        turn, inject reviewed context before inference, observe tool evidence,
        extract facts before older transcript is compacted, and queue memory
        extraction once a turn completes.
      </P>
    </>
  );
}

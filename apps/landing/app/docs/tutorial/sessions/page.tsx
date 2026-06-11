import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Strong, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Memory & sessions — codel00p docs",
  description:
    "Turn a chat into durable project memory, then name, resume, and inspect sessions.",
};

export default function TutorialSessionsPage() {
  return (
    <>
      <DocHeader
        group="Tutorial"
        title="Memory & sessions"
        lead="The point of codel00p is that one session makes the next one better. Here is how a conversation becomes durable memory, and how to pick it back up later."
      />

      <H2>Capture what it learned</H2>
      <P>
        After useful work, the run proposes <Strong>memory candidates</Strong>{" "}
        from what happened. Inside the chat, <Ic>/memory</Ic> shows the approved
        memory currently in context. To review the new candidates, open another
        terminal (or <Ic>/exit</Ic> first) and approve the ones worth keeping:
      </P>
      <Code>{`codel00p ...scope memory list
codel00p ...scope memory approve <memory-id>`}</Code>
      <P>
        Approved memory is loaded into every future chat automatically. Start a
        new session later and run <Ic>/memory</Ic> to see it already in context,
        so the agent does not relearn what you just taught it.
      </P>

      <H2>Name and resume a session</H2>
      <P>
        Every chat is saved under a session id, shown in the banner and via{" "}
        <Ic>/session</Ic>. Pass <Ic>--session-id</Ic> to give it a stable name,
        then resume later with the same id:
      </P>
      <Code>{`# start a named session
codel00p ...scope agent chat --session-id refactor-auth --tool-set all

# come back to it tomorrow
codel00p ...scope agent chat --session-id refactor-auth --tool-set all`}</Code>
      <P>
        On resume codel00p prints{" "}
        <Ic>Resumed conversation with N prior message(s)</Ic> and continues where
        you left off. Inside a chat, <Ic>/sessions</Ic> lists every saved
        conversation and <Ic>/reset</Ic> starts a fresh one without leaving.
      </P>

      <H2>Inspect sessions from outside</H2>
      <P>You do not need to be in a chat to look at past work:</P>
      <Code>{`codel00p ...scope session list
codel00p ...scope session show <session-id>`}</Code>
      <P>
        <Ic>session list</Ic> shows each conversation with its source, message
        count, and event count; <Ic>session show</Ic> prints the stored records.
        That is the full loop: chat, let it work, keep the memory, and resume.
        Next, read <Ic>Architecture</Ic> to see how a turn actually executes
        under the hood.
      </P>

      <Note>
        <p>
          Sessions and memory live in the SQLite file you pass with{" "}
          <Ic>--memory-db</Ic>, scoped to the organization and project flags. The
          file is yours; nothing is uploaded.
        </p>
      </Note>
    </>
  );
}

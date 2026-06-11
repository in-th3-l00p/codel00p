import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Strong, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Your first chat — codel00p docs",
  description:
    "Start an interactive multi-turn chat session with the codel00p CLI.",
};

export default function TutorialChatPage() {
  return (
    <>
      <DocHeader
        group="Tutorial"
        title="Your first chat"
        lead="The quick start ran a single turn. In practice you'll use the interactive chat — a multi-turn session that remembers the conversation, can use tools, and is saved so you can pick it back up."
      />

      <H2>Start a session</H2>
      <P>
        Set a provider key, then start <Ic>agent chat</Ic>. The four scope flags
        are the same as everywhere else; here they are written as{" "}
        <Ic>...scope</Ic> to stay readable.
      </P>
      <Code>{`export ANTHROPIC_API_KEY=sk-ant-...

codel00p ...scope agent chat \\
  --provider anthropic \\
  --model claude-opus-4-8`}</Code>
      <P>codel00p prints a banner and waits for you at a prompt:</P>
      <Code>{`codel00p chat — provider anthropic model claude-opus-4-8 (session 01JABC...)
Type a message and press Enter. Use /help for commands, /exit to quit.

you> What does this project do, and how is it organized?`}</Code>
      <P>
        Type a message and press Enter. The reply streams back token by token,
        then you land at another <Ic>you&gt;</Ic> prompt. The whole conversation
        stays in context, so your next message can build on the last answer
        without repeating yourself.
      </P>

      <H2>In-session commands</H2>
      <P>
        Anything starting with <Ic>/</Ic> is a command, not a message to the
        model:
      </P>
      <Code>{`/help            Show available commands
/history         Show the current conversation
/tools           List the tools available this turn
/model [id]      Show or switch the model for later turns
/memory          Show approved project memory in context
/session         Show the current session id
/sessions        List all saved conversations
/reset           Start a new conversation
/exit, /quit     Leave the chat`}</Code>
      <P>
        That is the whole loop: <Strong>talk</Strong>, let it{" "}
        <Strong>work</Strong>, and keep going. The next page gives the agent the
        tools to actually change your repository.
      </P>

      <Note>
        <p>
          Every command needs the scope flags (<Ic>--memory-db</Ic>,{" "}
          <Ic>--organization-id</Ic>, <Ic>--project-id</Ic>,{" "}
          <Ic>--project-name</Ic>) and a provider key in the environment. See{" "}
          <Ic>Installation</Ic> and <Ic>Quick start</Ic> if you have not set
          those up yet.
        </p>
      </Note>
    </>
  );
}

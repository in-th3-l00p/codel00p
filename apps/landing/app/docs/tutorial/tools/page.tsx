import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Strong, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Letting the agent work — codel00p docs",
  description:
    "Enable tools and permissions so the chat agent can read, edit, and run your project.",
};

export default function TutorialToolsPage() {
  return (
    <>
      <DocHeader
        group="Tutorial"
        title="Letting the agent work"
        lead="A chat is only useful if the agent can touch your repository. You decide which tools it gets and how much it can do without asking."
      />

      <H2>Enable tools</H2>
      <P>
        Start the chat with a tool set and a permission mode:
      </P>
      <Code>{`codel00p ...scope agent chat \\
  --provider anthropic \\
  --model claude-opus-4-8 \\
  --tool-set all \\
  --permission-mode ask`}</Code>
      <P>
        <Ic>--tool-set</Ic> chooses what the agent may use: <Ic>read</Ic>,{" "}
        <Ic>edit</Ic>, <Ic>command</Ic>, <Ic>git</Ic>, or <Ic>all</Ic>.{" "}
        <Ic>--permission-mode ask</Ic> confirms before sensitive actions, which
        is the right default while you build trust; <Ic>allow</Ic> runs without
        prompting and <Ic>deny</Ic> blocks tools entirely. Run <Ic>/tools</Ic>{" "}
        at any time to see what is available this turn.
      </P>

      <H2>A real turn</H2>
      <P>Ask for something that needs the repository, not just the model:</P>
      <Code>{`you> Run the test suite and tell me what is failing.`}</Code>
      <P>
        The agent calls the command tool. In <Ic>ask</Ic> mode you confirm it
        first; the output streams back and it summarizes the result. Because the
        conversation persists, you can follow straight up:
      </P>
      <Code>{`you> Fix the first failure, then re-run just that test.`}</Code>
      <P>
        It reads the relevant files, makes an edit, runs the single test, and
        reports back, all inside the same session. Every step is a{" "}
        <Strong>workspace-safe</Strong> tool call confined to your project
        directory.
      </P>

      <H2>Switch models mid-session</H2>
      <P>
        You do not need the strongest model for every turn. <Ic>/model</Ic>{" "}
        shows the current one; <Ic>/model &lt;id&gt;</Ic> switches it for later
        turns, so you can drop to a faster model for routine edits and come back
        up for tricky reasoning.
      </P>
      <Code>{`you> /model claude-haiku-4-5
Model set to claude-haiku-4-5.`}</Code>

      <Note>
        <p>
          Tools are confined to the workspace root (<Ic>--workspace</Ic>,
          defaulting to the current directory), so a run cannot read or write
          outside your project.
        </p>
      </Note>
    </>
  );
}

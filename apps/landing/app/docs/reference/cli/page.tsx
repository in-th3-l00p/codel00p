import type { Metadata } from "next";

import {
  DocHeader,
  H2,
  P,
  Code,
  Ic,
  Table,
  Note,
} from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "CLI commands — codel00p docs",
  description: "Global flags, commands, and the options for an agent run.",
};

export default function CliReferencePage() {
  return (
    <>
      <DocHeader
        group="Reference"
        title="CLI commands"
        lead="Every command takes the global scope flags first, then a command group and its options."
      />

      <Code>{`codel00p [global flags] <command> [options]`}</Code>

      <H2>Global flags</H2>
      <P>Required on every command, before the command group.</P>
      <Table
        head={["Flag", "Meaning"]}
        rows={[
          [<Ic key="a">--memory-db &lt;path&gt;</Ic>, "SQLite database for memory and sessions"],
          [<Ic key="b">--organization-id &lt;id&gt;</Ic>, "Organization scope"],
          [<Ic key="c">--project-id &lt;id&gt;</Ic>, "Project scope"],
          [<Ic key="d">--project-name &lt;name&gt;</Ic>, "Project display name"],
        ]}
      />

      <H2>Commands</H2>
      <Table
        head={["Command", "Purpose"]}
        rows={[
          [<Ic key="a">agent run</Ic>, "Run one agent turn"],
          [<Ic key="b">agent resume</Ic>, "Resume a persisted session"],
          [<Ic key="c">agent mcp</Ic>, "List or diagnose attached MCP servers"],
          [<Ic key="d">memory</Ic>, "Review, search, edit, and audit memory"],
          [<Ic key="e">session</Ic>, "Inspect persisted sessions"],
          [<Ic key="f">mcp serve</Ic>, "Run codel00p as an MCP server"],
          [<Ic key="g">mcp permissions</Ic>, "Inspect remembered connector decisions"],
        ]}
      />

      <H2>agent run options</H2>
      <Table
        head={["Option", "Meaning"]}
        rows={[
          [<Ic key="a">--provider &lt;id&gt;</Ic>, "Provider id or alias"],
          [<Ic key="b">--model &lt;id&gt;</Ic>, "Provider model id"],
          [<Ic key="c">--workspace &lt;path&gt;</Ic>, "Workspace root (defaults to cwd)"],
          [<Ic key="d">--tool-set &lt;name&gt;</Ic>, "read, edit, command, git, or all"],
          [<Ic key="e">--permission-mode &lt;mode&gt;</Ic>, "allow, ask, or deny"],
          [<Ic key="f">--session-id &lt;id&gt;</Ic>, "Persist under a stable session id"],
          [<Ic key="g">--max-iterations &lt;n&gt;</Ic>, "Cap model and tool iterations"],
          [<Ic key="h">--mcp-server &lt;id=cmd&gt;</Ic>, "Attach an MCP stdio server"],
          [<Ic key="i">--base-url &lt;url&gt;</Ic>, "Override the provider base URL"],
          [<Ic key="j">--provider-policy-preset &lt;id&gt;</Ic>, "Apply a built-in policy preset"],
          [<Ic key="k">--stream-events</Ic>, "Stream harness events during the turn"],
          [<Ic key="l">--json-events</Ic>, "Print serialized events after the answer"],
        ]}
      />

      <Note>
        <p>
          Append <Ic>--help</Ic> to any command or subcommand to see its full,
          current option list straight from the binary.
        </p>
      </Note>
    </>
  );
}

import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Strong } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Connecting MCP tools — codel00p docs",
  description:
    "Attach external MCP servers to a run, or expose codel00p as an MCP server.",
};

export default function McpGuidePage() {
  return (
    <>
      <DocHeader
        group="Guides"
        title="Connecting MCP tools"
        lead="codel00p speaks the Model Context Protocol in both directions: it can use external MCP tools, and it can serve its own memory and sessions to other clients."
      />

      <H2>Attach an external server</H2>
      <P>
        Pass <Ic>--mcp-server</Ic> as <Ic>name=command</Ic> to launch a stdio MCP
        server for a run. Its tools appear to the model as{" "}
        <Ic>mcp.&lt;name&gt;.&lt;tool&gt;</Ic> and run under the same permission
        checks as native tools.
      </P>
      <Code>{`codel00p ...scope agent run "List the open issues" \\
  --mcp-server github=mcp-server-github \\
  --tool-set all`}</Code>
      <P>
        A workspace can also declare servers in{" "}
        <Ic>.codel00p/mcp.json</Ic> so they load automatically.
      </P>

      <H2>Inspect connectors</H2>
      <P>
        Check which tools a server exposes without making a model call, and run
        redacted diagnostics if a connection misbehaves.
      </P>
      <Code>{`codel00p ...scope agent mcp list
codel00p ...scope agent mcp doctor`}</Code>

      <H2>Serve codel00p to other clients</H2>
      <P>
        Run codel00p as a stdio MCP server. It exposes project memory
        search, list, and show, reviewed candidate creation and review, and
        read-only session replay, plus JSON resources at{" "}
        <Ic>codel00p://memory/&#123;id&#125;</Ic> and{" "}
        <Ic>codel00p://sessions/&#123;id&#125;</Ic>.
      </P>
      <Code>{`codel00p ...scope mcp serve`}</Code>
      <P>
        Remembered <Strong>ask</Strong>-mode connector decisions can be reviewed
        with <Ic>mcp permissions</Ic>.
      </P>
    </>
  );
}

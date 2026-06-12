import Link from "next/link";
import { revalidatePath } from "next/cache";
import { auth } from "@clerk/nextjs/server";
import { Codel00pApiError, type Agent, type McpServer } from "@codel00p/sdk";

import { cloudClient } from "@/lib/api";
import { GlowBackground } from "@/components/site/glow-background";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export default async function ProjectDetailPage({
  params
}: {
  params: Promise<{ projectId: string }>;
}) {
  const { projectId } = await params;
  const { userId, redirectToSignIn, orgRole } = await auth();
  if (!userId) {
    return redirectToSignIn();
  }
  const isAdmin = orgRole === "org:admin" || orgRole === "admin";

  const client = await cloudClient();
  let projectName = projectId;
  let agents: Agent[] = [];
  let servers: McpServer[] = [];
  let error: string | null = null;
  try {
    const [project, agentList, serverList] = await Promise.all([
      client.getProject(projectId),
      client.listAgents(projectId),
      client.listMcpServers(projectId)
    ]);
    projectName = project.name;
    agents = agentList;
    servers = serverList;
  } catch (caught) {
    error =
      caught instanceof Codel00pApiError
        ? `${caught.status} ${caught.message}`
        : "Could not load this project. Is the cloud service running?";
  }

  return (
    <div className="relative min-h-screen">
      <GlowBackground />
      <main className="relative z-10 mx-auto flex max-w-3xl flex-col gap-6 px-6 py-10">
        <header>
          <Link
            href="/dashboard"
            className="label text-muted-foreground/70 transition-colors hover:text-foreground"
          >
            ← Dashboard
          </Link>
          <h1 className="mt-2 text-3xl font-medium tracking-tight text-foreground">
            {projectName}
          </h1>
        </header>

        {error ? (
          <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-sm text-destructive">
            {error}
          </p>
        ) : null}

        {/* Agents */}
        <Section title="Agents" count={agents.length}>
          {agents.length === 0 ? (
            <Empty text="No agents in the shared pool yet." />
          ) : (
            <ul className="flex flex-col gap-2">
              {agents.map((agent) => (
                <Row
                  key={agent.id}
                  title={agent.name}
                  subtitle={`${agent.provider}/${agent.model}${
                    agent.mcp_server_ids && agent.mcp_server_ids.length > 0
                      ? ` · ${agent.mcp_server_ids.length} mcp`
                      : ""
                  }`}
                  action={
                    isAdmin ? (
                      <form action={deleteAgent}>
                        <input type="hidden" name="id" value={agent.id} />
                        <LinkButton>delete</LinkButton>
                      </form>
                    ) : null
                  }
                />
              ))}
            </ul>
          )}

          {isAdmin ? (
            <form
              action={createAgent}
              className="mt-4 grid grid-cols-1 gap-2 sm:grid-cols-[1fr_1fr_1fr_auto]"
            >
              <Input name="name" placeholder="Agent name" required />
              <Input name="provider" placeholder="Provider (e.g. anthropic)" required />
              <Input name="model" placeholder="Model (e.g. claude-opus-4-8)" required />
              <Button type="submit" size="lg" className="rounded-full">
                Add agent
              </Button>
            </form>
          ) : (
            <p className="mt-3 text-xs text-muted-foreground">
              Only organization admins can manage agents.
            </p>
          )}
        </Section>

        {/* MCP servers */}
        <Section title="MCP servers" count={servers.length}>
          {servers.length === 0 ? (
            <Empty text="No MCP servers configured." />
          ) : (
            <ul className="flex flex-col gap-2">
              {servers.map((server) => (
                <Row
                  key={server.id}
                  title={
                    <>
                      {server.name}
                      {!server.enabled ? (
                        <span className="ml-2 text-xs text-muted-foreground">
                          (disabled)
                        </span>
                      ) : null}
                    </>
                  }
                  subtitle={`${server.transport}${
                    server.url ? ` · ${server.url}` : ""
                  }${server.command ? ` · ${server.command}` : ""}`}
                  action={
                    isAdmin ? (
                      <div className="flex items-center gap-3">
                        <form action={toggleMcp}>
                          <input type="hidden" name="id" value={server.id} />
                          <input
                            type="hidden"
                            name="enabled"
                            value={String(!server.enabled)}
                          />
                          <LinkButton>
                            {server.enabled ? "disable" : "enable"}
                          </LinkButton>
                        </form>
                        <form action={deleteMcp}>
                          <input type="hidden" name="id" value={server.id} />
                          <LinkButton>delete</LinkButton>
                        </form>
                      </div>
                    ) : null
                  }
                />
              ))}
            </ul>
          )}

          {isAdmin ? (
            <form
              action={createMcp}
              className="mt-4 grid grid-cols-1 gap-2 sm:grid-cols-[1fr_1fr_1fr_auto]"
            >
              <Input name="name" placeholder="Server name" required />
              <select
                name="transport"
                defaultValue="http"
                className="h-11 cursor-pointer rounded-lg border border-border bg-input/40 px-3.5 text-sm text-foreground outline-none focus-visible:border-ring/70 focus-visible:ring-3 focus-visible:ring-ring/40"
              >
                <option value="http">http</option>
                <option value="stdio">stdio</option>
              </select>
              <Input
                name="endpoint"
                placeholder="URL (http) or command (stdio)"
                required
              />
              <Button type="submit" size="lg" className="rounded-full">
                Add server
              </Button>
            </form>
          ) : null}
        </Section>
      </main>
    </div>
  );

  // --- Server actions (close over projectId) ---

  async function createAgent(formData: FormData) {
    "use server";
    const client = await cloudClient();
    await client.createAgent(projectId, {
      name: String(formData.get("name") ?? "").trim(),
      provider: String(formData.get("provider") ?? "").trim(),
      model: String(formData.get("model") ?? "").trim()
    });
    revalidatePath(`/projects/${projectId}`);
  }

  async function deleteAgent(formData: FormData) {
    "use server";
    const client = await cloudClient();
    await client.deleteAgent(projectId, String(formData.get("id")));
    revalidatePath(`/projects/${projectId}`);
  }

  async function createMcp(formData: FormData) {
    "use server";
    const transport = String(formData.get("transport")) === "stdio" ? "stdio" : "http";
    const endpoint = String(formData.get("endpoint") ?? "").trim();
    const client = await cloudClient();
    await client.createMcpServer(projectId, {
      name: String(formData.get("name") ?? "").trim(),
      transport,
      url: transport === "http" ? endpoint : undefined,
      command: transport === "stdio" ? endpoint : undefined
    });
    revalidatePath(`/projects/${projectId}`);
  }

  async function toggleMcp(formData: FormData) {
    "use server";
    const client = await cloudClient();
    await client.updateMcpServer(projectId, String(formData.get("id")), {
      enabled: String(formData.get("enabled")) === "true"
    });
    revalidatePath(`/projects/${projectId}`);
  }

  async function deleteMcp(formData: FormData) {
    "use server";
    const client = await cloudClient();
    await client.deleteMcpServer(projectId, String(formData.get("id")));
    revalidatePath(`/projects/${projectId}`);
  }
}

function Section({
  title,
  count,
  children
}: {
  title: string;
  count: number;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-2xl border border-border bg-card/40 p-6 backdrop-blur-sm">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-medium tracking-tight text-foreground">
          {title}
        </h2>
        <span className="rounded-full border border-border px-2.5 py-0.5 font-mono text-xs text-muted-foreground">
          {count}
        </span>
      </div>
      {children}
    </section>
  );
}

function Row({
  title,
  subtitle,
  action
}: {
  title: React.ReactNode;
  subtitle: string;
  action?: React.ReactNode;
}) {
  return (
    <li className="flex items-center justify-between gap-4 rounded-xl border border-border bg-card/30 px-4 py-3">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium text-foreground">{title}</p>
        <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
          {subtitle}
        </p>
      </div>
      {action}
    </li>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="text-sm text-muted-foreground">{text}</p>;
}

function LinkButton({ children }: { children: React.ReactNode }) {
  return (
    <button
      type="submit"
      className="text-xs text-muted-foreground transition-colors hover:text-destructive"
    >
      {children}
    </button>
  );
}

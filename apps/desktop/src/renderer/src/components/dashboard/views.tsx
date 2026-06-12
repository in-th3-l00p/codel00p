import type { DashboardData } from "@/lib/use-dashboard-data";
import {
  matchesScope,
  type DashboardView,
  type ScopeFilter
} from "@/lib/dashboard-types";
import {
  DataRow,
  EmptyState,
  SectionHeading,
  SourceBadge,
  StatCard
} from "./primitives";
import { InstallEnginePanel } from "./install-engine";

export function DashboardContent({
  view,
  scope,
  data
}: {
  view: DashboardView;
  scope: ScopeFilter;
  data: DashboardData;
}) {
  switch (view) {
    case "overview":
      return <OverviewView scope={scope} data={data} />;
    case "organizations":
      return <OrganizationsView scope={scope} data={data} />;
    case "projects":
      return <ProjectsView scope={scope} data={data} />;
    case "agents":
      return <AgentsView scope={scope} data={data} />;
  }
}

function OverviewView({
  scope,
  data
}: {
  scope: ScopeFilter;
  data: DashboardData;
}) {
  const orgs = data.orgs.filter((item) => matchesScope(item.source, scope));
  const projects = data.projects.filter((item) => matchesScope(item.source, scope));
  const agents = data.agents.filter((item) => matchesScope(item.source, scope));

  return (
    <div className="rise flex flex-col gap-8">
      <div className="grid grid-cols-3 gap-4">
        <StatCard
          label="Organizations"
          value={orgs.length}
          hint={`${countBy(orgs, "cloud")} cloud · ${countBy(orgs, "local")} local`}
        />
        <StatCard
          label="Projects"
          value={projects.length}
          hint={`${countBy(projects, "cloud")} cloud · ${countBy(projects, "local")} local`}
        />
        <StatCard
          label="Agents"
          value={agents.length}
          hint={`${countBy(agents, "cloud")} cloud · ${countBy(agents, "local")} local`}
        />
      </div>

      {data.cloud.error ? (
        <Notice
          tone="info"
          text={`Cloud: ${data.cloud.error}. Select an organization or start the cloud service.`}
        />
      ) : null}

      {!data.local.binaryFound && scope !== "cloud" ? (
        <InstallEnginePanel />
      ) : (
        <div>
          <SectionHeading>Recent agents</SectionHeading>
          <AgentList agents={agents.slice(0, 5)} />
        </div>
      )}
    </div>
  );
}

function OrganizationsView({
  scope,
  data
}: {
  scope: ScopeFilter;
  data: DashboardData;
}) {
  const orgs = data.orgs.filter((item) => matchesScope(item.source, scope));
  if (orgs.length === 0) {
    return (
      <EmptyState
        title="No organizations in view"
        body="Cloud organizations come from your Clerk memberships; the local workspace appears when the engine is connected."
      />
    );
  }
  return (
    <ul className="rise flex flex-col gap-2">
      {orgs.map((org) => (
        <DataRow
          key={`${org.source}:${org.id}`}
          title={org.name}
          subtitle={org.id}
          badge={<SourceBadge source={org.source} />}
          meta={org.role ? <span>{org.role}</span> : null}
        />
      ))}
    </ul>
  );
}

function ProjectsView({
  scope,
  data
}: {
  scope: ScopeFilter;
  data: DashboardData;
}) {
  const projects = data.projects.filter((item) => matchesScope(item.source, scope));
  if (projects.length === 0) {
    return (
      <EmptyState
        title="No projects in view"
        body="Cloud projects belong to the active organization; switch organizations in the top bar to see others."
      />
    );
  }
  return (
    <ul className="rise flex flex-col gap-2">
      {projects.map((project) => (
        <DataRow
          key={`${project.source}:${project.id}`}
          title={project.name}
          subtitle={project.slug ?? project.id}
          badge={<SourceBadge source={project.source} />}
          meta={
            project.repositoryUrl ? (
              <span className="font-mono">{shortRepo(project.repositoryUrl)}</span>
            ) : null
          }
        />
      ))}
    </ul>
  );
}

function AgentsView({
  scope,
  data
}: {
  scope: ScopeFilter;
  data: DashboardData;
}) {
  const agents = data.agents.filter((item) => matchesScope(item.source, scope));
  const showInstall = scope === "local" && !data.local.binaryFound;
  return (
    <div className="rise flex flex-col gap-4">
      <Notice
        tone="muted"
        text="Cloud shows the organization's shared agent pool; Local shows agent runs (sessions) on this machine."
      />
      {showInstall ? <InstallEnginePanel /> : <AgentList agents={agents} />}
    </div>
  );
}

function AgentList({ agents }: { agents: DashboardData["agents"] }) {
  if (agents.length === 0) {
    return (
      <EmptyState
        title="No agents yet"
        body="Agent runs appear here as sessions. Start one with `codel00p agent chat`."
      />
    );
  }
  return (
    <ul className="flex flex-col gap-2">
      {agents.map((agent) => (
        <DataRow
          key={`${agent.source}:${agent.id}`}
          title={agent.label}
          subtitle={`source: ${agent.origin}`}
          badge={<SourceBadge source={agent.source} />}
          meta={
            <span>
              {agent.messages ?? 0} msg · {agent.events ?? 0} evt
            </span>
          }
        />
      ))}
    </ul>
  );
}

function Notice({ tone, text }: { tone: "info" | "muted"; text: string }) {
  return (
    <p
      className={
        tone === "info"
          ? "rounded-lg border border-brand/25 bg-brand/5 px-3.5 py-2.5 text-xs text-brand"
          : "rounded-lg border border-border bg-card/30 px-3.5 py-2.5 text-xs text-muted-foreground"
      }
    >
      {text}
    </p>
  );
}

function countBy(items: { source: string }[], source: string): number {
  return items.filter((item) => item.source === source).length;
}

function shortRepo(url: string): string {
  return url.replace(/^https?:\/\/(www\.)?/, "").replace(/\.git$/, "");
}

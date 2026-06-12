import type { DashboardData, DashboardView } from "@/lib/dashboard-types";
import {
  DataRow,
  EmptyState,
  Notice,
  SectionHeading,
  SourceBadge,
  StatCard
} from "./primitives";
import { NewProjectForm } from "./new-project-form";

export function DashboardContent({
  view,
  data
}: {
  view: DashboardView;
  data: DashboardData;
}) {
  switch (view) {
    case "overview":
      return <OverviewView data={data} />;
    case "organizations":
      return <OrganizationsView data={data} />;
    case "projects":
      return <ProjectsView data={data} />;
    case "agents":
      return <AgentsView data={data} />;
  }
}

function OverviewView({ data }: { data: DashboardData }) {
  return (
    <div className="rise flex flex-col gap-8">
      <div className="grid grid-cols-3 gap-4">
        <StatCard label="Organizations" value={data.orgs.length} />
        <StatCard label="Projects" value={data.projects.length} />
        <StatCard label="Agents" value={data.agents.length} />
      </div>

      {data.cloudError ? (
        <Notice
          tone="info"
          text={`Cloud: ${data.cloudError}. Select an organization or start the cloud service.`}
        />
      ) : !data.hasOrg ? (
        <Notice
          tone="muted"
          text="No organization is active. Create or select one from the switcher above to manage its projects and agents."
        />
      ) : null}

      <div>
        <SectionHeading>Recent agents</SectionHeading>
        <AgentList agents={data.agents.slice(0, 5)} />
      </div>
    </div>
  );
}

function OrganizationsView({ data }: { data: DashboardData }) {
  if (data.orgs.length === 0) {
    return (
      <EmptyState
        title="No organizations in view"
        body="Organizations come from your Clerk memberships. Create one from the switcher in the top bar."
      />
    );
  }
  return (
    <ul className="rise flex flex-col gap-2">
      {data.orgs.map((org) => (
        <DataRow
          key={org.id}
          title={org.name}
          subtitle={org.id}
          badge={<SourceBadge source={org.source} />}
          meta={org.role ? <span>{org.role}</span> : null}
        />
      ))}
    </ul>
  );
}

function ProjectsView({ data }: { data: DashboardData }) {
  return (
    <div className="rise flex flex-col gap-6">
      {data.projects.length === 0 ? (
        <EmptyState
          title="No projects yet"
          body={
            data.isAdmin
              ? "Create the org's first project below, then open it to add agents and MCP servers."
              : "Projects belong to the active organization. Only org admins can create them."
          }
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {data.projects.map((project) => (
            <DataRow
              key={project.id}
              href={`/projects/${project.id}`}
              title={project.name}
              subtitle={project.slug ?? project.id}
              badge={<SourceBadge source={project.source} />}
              meta={
                project.repositoryUrl ? (
                  <span className="font-mono">{shortRepo(project.repositoryUrl)}</span>
                ) : (
                  <span aria-hidden>manage →</span>
                )
              }
            />
          ))}
        </ul>
      )}

      {data.isAdmin && data.hasOrg ? (
        <div>
          <SectionHeading>New project</SectionHeading>
          <NewProjectForm />
        </div>
      ) : null}
    </div>
  );
}

function AgentsView({ data }: { data: DashboardData }) {
  return (
    <div className="rise flex flex-col gap-4">
      <Notice
        tone="muted"
        text="The organization's shared agent pool across all of its projects."
      />
      <AgentList agents={data.agents} />
    </div>
  );
}

function AgentList({ agents }: { agents: DashboardData["agents"] }) {
  if (agents.length === 0) {
    return (
      <EmptyState
        title="No agents yet"
        body="Define an agent on a project to add it to the shared pool."
      />
    );
  }
  return (
    <ul className="flex flex-col gap-2">
      {agents.map((agent) => (
        <DataRow
          key={agent.id}
          title={agent.label}
          subtitle={`provider: ${agent.origin}`}
          badge={<SourceBadge source={agent.source} />}
        />
      ))}
    </ul>
  );
}

function shortRepo(url: string): string {
  return url.replace(/^https?:\/\/(www\.)?/, "").replace(/\.git$/, "");
}

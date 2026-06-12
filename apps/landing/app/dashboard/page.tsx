import { redirect } from "next/navigation";
import { auth, clerkClient } from "@clerk/nextjs/server";
import { Codel00pApiError, type Viewer } from "@codel00p/sdk";

import { cloudClient } from "@/lib/api";
import { DashboardShell } from "@/components/dashboard/dashboard-shell";
import type {
  AgentItem,
  DashboardData,
  OrgItem,
  ProjectItem
} from "@/lib/dashboard-types";

export default async function DashboardPage() {
  const { userId, orgRole } = await auth();
  if (!userId) {
    redirect("/sign-in");
  }
  const isAdmin = orgRole === "org:admin" || orgRole === "admin";

  const client = await cloudClient();

  let viewer: Viewer | null = null;
  let projects: ProjectItem[] = [];
  let agents: AgentItem[] = [];
  let cloudError: string | undefined;

  try {
    viewer = await client.me();
    if (viewer.org) {
      const projectList = await client.listProjects();
      projects = projectList.map((project) => ({
        id: project.id,
        name: project.name,
        source: "cloud" as const,
        slug: project.slug,
        repositoryUrl: project.repository_url
      }));

      // Agents are project-scoped; gather the active org's shared pool.
      const agentLists = await Promise.all(
        projectList.map((project) =>
          client.listAgents(project.id).catch(() => [])
        )
      );
      agents = agentLists.flat().map((agent) => ({
        id: agent.id,
        source: "cloud" as const,
        label: agent.name,
        origin: `${agent.provider}/${agent.model}`
      }));
    }
  } catch (error) {
    cloudError =
      error instanceof Codel00pApiError
        ? `${error.status} ${error.message}`
        : "The cloud service is unreachable. Is codel00p-cloud running on CODEL00P_API_URL?";
  }

  // Organizations come from the viewer's Clerk memberships.
  let orgs: OrgItem[] = [];
  try {
    const clerk = await clerkClient();
    const memberships = await clerk.users.getOrganizationMembershipList({
      userId
    });
    orgs = memberships.data.map((membership) => ({
      id: membership.organization.id,
      name: membership.organization.name,
      source: "cloud" as const,
      role: roleLabel(membership.role)
    }));
  } catch {
    // Non-fatal: the dashboard still renders with whatever cloud data loaded.
  }

  const data: DashboardData = {
    orgs,
    projects,
    agents,
    hasOrg: Boolean(viewer?.org),
    isAdmin,
    cloudError
  };

  return <DashboardShell data={data} />;
}

function roleLabel(role: string): string {
  const normalized = role.replace(/^org:/, "");
  return normalized === "basic_member" ? "member" : normalized;
}

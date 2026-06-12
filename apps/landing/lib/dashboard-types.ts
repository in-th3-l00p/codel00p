/**
 * Shapes the dashboard renders. The web control surface is cloud-only (the
 * local engine lives in the desktop app), but we keep the `source` discriminant
 * so the visual language — and the components — stay shared with the desktop.
 */
export type Source = "cloud" | "local";

/** Which entity the dashboard is visualizing. */
export type DashboardView = "overview" | "organizations" | "projects" | "agents";

export type OrgItem = {
  id: string;
  name: string;
  source: Source;
  role?: string;
};

export type ProjectItem = {
  id: string;
  name: string;
  source: Source;
  slug?: string;
  repositoryUrl?: string;
};

export type AgentItem = {
  id: string;
  source: Source;
  label: string;
  origin: string;
  messages?: number;
  events?: number;
};

/** The fully-resolved snapshot the server hands to the client shell. */
export type DashboardData = {
  orgs: OrgItem[];
  projects: ProjectItem[];
  agents: AgentItem[];
  /** Whether an organization is active for the viewer. */
  hasOrg: boolean;
  /** Whether the viewer is an admin of the active org (gates create actions). */
  isAdmin: boolean;
  /** Set when the cloud service couldn't be reached or returned an error. */
  cloudError?: string;
};

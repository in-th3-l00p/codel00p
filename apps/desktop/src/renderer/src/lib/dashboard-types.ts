/** Where a piece of data lives: the local engine or the team cloud. */
export type Source = "local" | "cloud";

/** Sidebar scope filter: a single source or both. */
export type ScopeFilter = "all" | Source;

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
  org?: string;
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

export function matchesScope(source: Source, scope: ScopeFilter): boolean {
  return scope === "all" || scope === source;
}

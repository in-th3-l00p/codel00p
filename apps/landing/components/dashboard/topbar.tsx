"use client";

import { OrganizationSwitcher, UserButton } from "@clerk/nextjs";

import type { DashboardView } from "@/lib/dashboard-types";

const TITLES: Record<DashboardView, string> = {
  overview: "Overview",
  organizations: "Organizations",
  projects: "Projects",
  agents: "Agents"
};

export function Topbar({
  view,
  onRefresh,
  refreshing
}: {
  view: DashboardView;
  onRefresh: () => void;
  refreshing: boolean;
}) {
  return (
    <header className="relative z-10 flex items-center justify-between border-b border-border px-8 py-4">
      <div>
        <p className="label text-muted-foreground/70">Dashboard</p>
        <h1 className="mt-1 text-xl font-medium tracking-tight text-foreground">
          {TITLES[view]}
        </h1>
      </div>

      <div className="flex items-center gap-4">
        <button
          type="button"
          onClick={onRefresh}
          disabled={refreshing}
          className="rounded-full border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground disabled:opacity-50"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>

        <OrganizationSwitcher
          hidePersonal
          afterSelectOrganizationUrl="/dashboard"
          afterCreateOrganizationUrl="/dashboard"
        />
        <UserButton />
      </div>
    </header>
  );
}

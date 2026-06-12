"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";

import { GlowBackground } from "@/components/site/glow-background";
import type { DashboardData, DashboardView } from "@/lib/dashboard-types";
import { Sidebar } from "./sidebar";
import { Topbar } from "./topbar";
import { DashboardContent } from "./views";

/**
 * The web dashboard shell. Data is resolved on the server and handed in; this
 * client layer owns only which view is active and re-running the server load on
 * refresh. Mirrors the desktop dashboard, scoped to the team cloud.
 */
export function DashboardShell({ data }: { data: DashboardData }) {
  const [view, setView] = useState<DashboardView>("overview");
  const [isPending, startTransition] = useTransition();
  const router = useRouter();

  const counts: Record<DashboardView, number | null> = {
    overview: null,
    organizations: data.orgs.length,
    projects: data.projects.length,
    agents: data.agents.length
  };

  function refresh() {
    startTransition(() => router.refresh());
  }

  return (
    <div className="relative grid min-h-screen grid-cols-[240px_1fr]">
      <GlowBackground />
      <div className="sticky top-0 h-screen">
        <Sidebar view={view} onView={setView} counts={counts} />
      </div>
      <div className="flex min-w-0 flex-col">
        <Topbar view={view} onRefresh={refresh} refreshing={isPending} />
        <main className="relative z-10 flex-1 px-8 py-7">
          <DashboardContent view={view} data={data} />
        </main>
      </div>
    </div>
  );
}

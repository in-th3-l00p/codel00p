import { useState } from "react";

import { GlowBackground } from "@/components/site/glow-background";
import { useDashboardData } from "@/lib/use-dashboard-data";
import type { DashboardView, ScopeFilter } from "@/lib/dashboard-types";
import { Sidebar } from "./sidebar";
import { Topbar } from "./topbar";
import { DashboardContent } from "./views";

/**
 * The desktop dashboard: a read-only overview of organizations, projects, and
 * agents across the local engine and the team cloud. The sidebar switches what
 * to visualize; the top bar filters by source and switches the active org.
 */
export function Dashboard() {
  const [view, setView] = useState<DashboardView>("overview");
  const [scope, setScope] = useState<ScopeFilter>("all");
  const data = useDashboardData();

  return (
    <div className="relative grid h-full grid-cols-[240px_1fr] overflow-hidden pt-9">
      <GlowBackground />
      <Sidebar view={view} onView={setView} data={data} />
      <div className="flex min-w-0 flex-col">
        <Topbar view={view} scope={scope} onScope={setScope} live={data.live} />
        <main className="relative z-10 min-h-0 flex-1 overflow-y-auto px-8 py-7">
          <DashboardContent view={view} scope={scope} data={data} />
        </main>
      </div>
    </div>
  );
}

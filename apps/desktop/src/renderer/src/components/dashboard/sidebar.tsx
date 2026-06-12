import { LoopMark } from "@/components/site/loop-mark";
import { cn } from "@/lib/utils";
import type { DashboardData } from "@/lib/use-dashboard-data";
import type { DashboardView } from "@/lib/dashboard-types";

const NAV: { key: DashboardView; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "organizations", label: "Organizations" },
  { key: "projects", label: "Projects" },
  { key: "agents", label: "Agents" }
];

export function Sidebar({
  view,
  onView,
  data
}: {
  view: DashboardView;
  onView: (view: DashboardView) => void;
  data: DashboardData;
}) {
  const counts: Record<DashboardView, number | null> = {
    overview: null,
    organizations: data.orgs.length,
    projects: data.projects.length,
    agents: data.agents.length
  };

  return (
    <aside className="relative z-10 flex h-full flex-col border-r border-border bg-card/30 backdrop-blur-sm">
      <div className="flex items-center gap-2.5 px-5 py-5">
        <LoopMark className="size-7" />
        <span className="font-hand text-2xl leading-none text-foreground">
          codel00p
        </span>
      </div>

      <nav className="flex flex-col gap-1 px-3 py-2">
        {NAV.map((item) => (
          <button
            key={item.key}
            type="button"
            onClick={() => onView(item.key)}
            data-active={view === item.key}
            className={cn(
              "group flex items-center justify-between rounded-lg px-3 py-2 text-sm transition-colors",
              "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
              "data-[active=true]:bg-foreground/[0.08] data-[active=true]:text-foreground"
            )}
          >
            <span>{item.label}</span>
            {counts[item.key] !== null ? (
              <span className="font-mono text-xs text-muted-foreground/60">
                {counts[item.key]}
              </span>
            ) : null}
          </button>
        ))}
      </nav>

      <div className="mt-auto flex flex-col gap-2 px-5 py-5 text-xs text-muted-foreground">
        <span className="flex items-center gap-2">
          <span className="size-1.5 rounded-full bg-brand" />
          Cloud · team knowledge
        </span>
        <span className="flex items-center gap-2">
          <span className="size-1.5 rounded-full bg-muted-foreground/60" />
          {data.local.available ? "Local · this machine" : "Local · not connected"}
        </span>
      </div>
    </aside>
  );
}

import { cn } from "@/lib/utils";
import type { DashboardView, ScopeFilter } from "@/lib/dashboard-types";
import { OrgMenu, UserMenu } from "./account-menus";

const TITLES: Record<DashboardView, string> = {
  overview: "Overview",
  organizations: "Organizations",
  projects: "Projects",
  agents: "Agents"
};

const SCOPES: { key: ScopeFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "cloud", label: "Cloud" },
  { key: "local", label: "Local" }
];

export function Topbar({
  view,
  scope,
  onScope,
  live
}: {
  view: DashboardView;
  scope: ScopeFilter;
  onScope: (scope: ScopeFilter) => void;
  live: boolean;
}) {
  return (
    <header className="app-drag relative z-10 flex items-center justify-between border-b border-border px-8 py-4">
      <div>
        <p className="label flex items-center gap-2 text-muted-foreground/70">
          Dashboard
          <span
            title={live ? "Live updates connected" : "Connecting…"}
            className={cn(
              "size-1.5 rounded-full",
              live ? "bg-emerald-400" : "bg-muted-foreground/50"
            )}
          />
        </p>
        <h1 className="mt-1 text-xl font-medium tracking-tight text-foreground">
          {TITLES[view]}
        </h1>
      </div>

      <div className="app-no-drag flex items-center gap-4">
        <div className="flex items-center rounded-full border border-border bg-card/40 p-0.5">
          {SCOPES.map((option) => (
            <button
              key={option.key}
              type="button"
              onClick={() => onScope(option.key)}
              data-active={scope === option.key}
              className={cn(
                "rounded-full px-3 py-1 text-xs font-medium transition-colors",
                "text-muted-foreground hover:text-foreground",
                "data-[active=true]:bg-foreground data-[active=true]:text-background"
              )}
            >
              {option.label}
            </button>
          ))}
        </div>

        <OrgMenu />
        <UserMenu />
      </div>
    </header>
  );
}

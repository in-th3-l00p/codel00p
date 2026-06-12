import type { ReactNode } from "react";
import Link from "next/link";

import { cn } from "@/lib/utils";
import type { Source } from "@/lib/dashboard-types";

export function SourceBadge({ source }: { source: Source }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 font-mono text-[0.65rem] uppercase tracking-wider",
        source === "cloud"
          ? "border-brand/30 text-brand"
          : "border-border text-muted-foreground"
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          source === "cloud" ? "bg-brand" : "bg-muted-foreground/60"
        )}
      />
      {source}
    </span>
  );
}

export function StatCard({
  label,
  value,
  hint
}: {
  label: string;
  value: number | string;
  hint?: ReactNode;
}) {
  return (
    <div className="rounded-2xl border border-border bg-card/40 p-5 backdrop-blur-sm">
      <p className="label text-muted-foreground/70">{label}</p>
      <p className="mt-3 text-3xl font-medium tracking-tight text-foreground">
        {value}
      </p>
      {hint ? (
        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  );
}

export function DataRow({
  title,
  subtitle,
  badge,
  meta,
  href
}: {
  title: string;
  subtitle?: string;
  badge?: ReactNode;
  meta?: ReactNode;
  href?: string;
}) {
  const inner = (
    <>
      <div className="min-w-0">
        <div className="flex items-center gap-2.5">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          {badge}
        </div>
        {subtitle ? (
          <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
            {subtitle}
          </p>
        ) : null}
      </div>
      {meta ? (
        <div className="shrink-0 text-right text-xs text-muted-foreground">{meta}</div>
      ) : null}
    </>
  );

  const className =
    "flex items-center justify-between gap-4 rounded-xl border border-border bg-card/30 px-4 py-3 transition-colors hover:bg-card/60";

  if (href) {
    return (
      <li>
        <Link href={href} className={cn(className, "group")}>
          {inner}
        </Link>
      </li>
    );
  }

  return <li className={className}>{inner}</li>;
}

export function EmptyState({ title, body }: { title: string; body?: string }) {
  return (
    <div className="rounded-2xl border border-dashed border-border bg-card/20 px-6 py-12 text-center">
      <p className="text-sm font-medium text-foreground">{title}</p>
      {body ? (
        <p className="mx-auto mt-2 max-w-sm text-xs leading-relaxed text-muted-foreground">
          {body}
        </p>
      ) : null}
    </div>
  );
}

export function SectionHeading({ children }: { children: ReactNode }) {
  return <p className="label mb-3 text-muted-foreground/70">{children}</p>;
}

export function Notice({ tone, text }: { tone: "info" | "muted"; text: string }) {
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

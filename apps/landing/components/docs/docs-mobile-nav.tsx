"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import { Menu, X } from "lucide-react";

import { docsPages, groupFor } from "@/components/docs/nav";
import { DocsNavTree } from "@/components/docs/docs-nav-tree";
import { LoopMark } from "@/components/site/loop-mark";

/** Trigger bar plus slide-in drawer shown below the lg breakpoint. */
export function DocsMobileNav() {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);

  const current = docsPages.find((page) => page.href === pathname);
  const group = groupFor(pathname);

  // Close on route change.
  useEffect(() => {
    setOpen(false);
  }, [pathname]);

  // Lock body scroll and wire Escape while the drawer is open.
  useEffect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      document.body.style.overflow = previous;
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="lg:hidden">
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-expanded={open}
        aria-haspopup="dialog"
        className="flex w-full items-center justify-between rounded-xl border border-border bg-card/40 px-4 py-3 text-sm transition-colors hover:bg-card/60"
      >
        <span className="flex items-center gap-2 text-muted-foreground">
          <Menu className="size-4" />
          Menu
        </span>
        <span className="truncate pl-3 text-right">
          {group ? (
            <span className="text-muted-foreground/60">{group} / </span>
          ) : null}
          <span className="font-medium text-foreground">
            {current?.title ?? "Docs"}
          </span>
        </span>
      </button>

      {/* Drawer */}
      <div
        className={open ? "fixed inset-0 z-50" : "pointer-events-none fixed inset-0 z-50"}
        aria-hidden={!open}
      >
        <div
          onClick={() => setOpen(false)}
          className={`absolute inset-0 bg-background/80 backdrop-blur-sm transition-opacity duration-300 ${
            open ? "opacity-100" : "opacity-0"
          }`}
        />
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Documentation navigation"
          className={`absolute inset-y-0 left-0 flex w-[84%] max-w-xs flex-col border-r border-border bg-background shadow-2xl transition-transform duration-300 ease-out ${
            open ? "translate-x-0" : "-translate-x-full"
          }`}
        >
          <div className="flex items-center justify-between border-b border-border/60 px-5 py-4">
            <span className="flex items-center gap-2.5">
              <LoopMark className="size-5" />
              <span className="font-mono text-sm text-foreground/90">
                codel00p
              </span>
            </span>
            <button
              type="button"
              onClick={() => setOpen(false)}
              aria-label="Close navigation"
              className="rounded-md p-1 text-muted-foreground transition-colors hover:text-foreground"
            >
              <X className="size-5" />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto px-5 py-6">
            <DocsNavTree onNavigate={() => setOpen(false)} />
          </div>
        </div>
      </div>
    </div>
  );
}

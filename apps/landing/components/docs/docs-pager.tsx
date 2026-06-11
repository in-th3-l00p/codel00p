"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { ArrowLeft, ArrowRight } from "lucide-react";

import { docsPages } from "@/components/docs/nav";

export function DocsPager() {
  const pathname = usePathname();
  const index = docsPages.findIndex((page) => page.href === pathname);
  const prev = index > 0 ? docsPages[index - 1] : null;
  const next =
    index >= 0 && index < docsPages.length - 1 ? docsPages[index + 1] : null;

  return (
    <nav className="mt-16 grid grid-cols-2 gap-4 border-t border-border/60 pt-8">
      {prev ? (
        <Link
          href={prev.href}
          className="group flex flex-col gap-1 rounded-xl border border-border bg-card/30 px-4 py-3 transition-colors hover:bg-card/60"
        >
          <span className="label flex items-center gap-1.5 text-muted-foreground/60">
            <ArrowLeft className="size-3" />
            Previous
          </span>
          <span className="text-sm font-medium text-foreground">
            {prev.title}
          </span>
        </Link>
      ) : (
        <span />
      )}
      {next ? (
        <Link
          href={next.href}
          className="group flex flex-col items-end gap-1 rounded-xl border border-border bg-card/30 px-4 py-3 text-right transition-colors hover:bg-card/60"
        >
          <span className="label flex items-center gap-1.5 text-muted-foreground/60">
            Next
            <ArrowRight className="size-3" />
          </span>
          <span className="text-sm font-medium text-foreground">
            {next.title}
          </span>
        </Link>
      ) : (
        <span />
      )}
    </nav>
  );
}

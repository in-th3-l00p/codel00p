import type { ReactNode } from "react";

import { GlowBackground } from "@/components/site/glow-background";
import { SiteHeader } from "@/components/site/site-header";
import { SiteFooter } from "@/components/site/site-footer";
import { DocsNavTree } from "@/components/docs/docs-nav-tree";
import { DocsMobileNav } from "@/components/docs/docs-mobile-nav";
import { DocsPager } from "@/components/docs/docs-pager";

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="relative flex min-h-dvh flex-col">
      <GlowBackground />
      <SiteHeader />

      {/*
        Mobile chapter nav, sticky under the header. Note: no backdrop-blur /
        transform here — a filtered ancestor would become the containing block
        for the drawer's `fixed` overlay and trap it inside this bar.
      */}
      <div className="sticky top-0 z-30 border-b border-border/60 bg-background/95 lg:hidden">
        <div className="mx-auto w-full max-w-6xl px-6 py-3">
          <DocsMobileNav />
        </div>
      </div>

      <div className="mx-auto w-full max-w-6xl flex-1 px-6 pb-24 pt-8 lg:pt-10">
        <div className="lg:grid lg:grid-cols-[224px_minmax(0,1fr)] lg:gap-12">
          <aside className="hidden lg:block">
            <div className="sticky top-10 max-h-[calc(100dvh-5rem)] overflow-y-auto pb-10">
              <DocsNavTree />
            </div>
          </aside>

          <div className="min-w-0">
            <article className="mx-auto max-w-[72ch]">{children}</article>
            <div className="mx-auto max-w-[72ch]">
              <DocsPager />
            </div>
          </div>
        </div>
      </div>

      <SiteFooter />
    </div>
  );
}

import type { ReactNode } from "react";

import { GlowBackground } from "@/components/site/glow-background";
import { SiteHeader } from "@/components/site/site-header";
import { SiteFooter } from "@/components/site/site-footer";
import { DocsSidebar } from "@/components/docs/docs-sidebar";
import { DocsPager } from "@/components/docs/docs-pager";

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="relative flex min-h-dvh flex-col">
      <GlowBackground />
      <SiteHeader />

      <div className="mx-auto w-full max-w-6xl flex-1 px-6 pb-24 pt-10">
        <div className="lg:grid lg:grid-cols-[224px_minmax(0,1fr)] lg:gap-12">
          <aside className="hidden lg:block">
            <div className="sticky top-10 max-h-[calc(100dvh-5rem)] overflow-y-auto pb-10">
              <DocsSidebar />
            </div>
          </aside>

          <div className="min-w-0">
            <article className="max-w-[72ch]">{children}</article>
            <div className="max-w-[72ch]">
              <DocsPager />
            </div>
          </div>
        </div>
      </div>

      <SiteFooter />
    </div>
  );
}

import Link from "next/link";

import { Button } from "@/components/ui/button";
import { LoopMark } from "@/components/site/loop-mark";

const REPO_URL = "https://github.com/inth3loop/codel00p";

export function SiteHeader() {
  return (
    <header className="relative z-10">
      <div className="mx-auto flex w-full max-w-5xl items-center justify-between px-6 py-6">
        <Link href="/" className="flex items-center gap-2.5">
          <LoopMark className="size-6" />
          <span className="font-mono text-sm tracking-tight text-foreground/90">
            codel00p
          </span>
        </Link>
        <nav className="flex items-center gap-1">
          <Button
            asChild
            variant="ghost"
            className="h-8 px-3 text-muted-foreground hover:text-foreground"
          >
            <Link href="/docs">Docs</Link>
          </Button>
          <Button
            asChild
            variant="ghost"
            className="h-8 px-3 text-muted-foreground hover:text-foreground"
          >
            <a href={REPO_URL} target="_blank" rel="noreferrer">
              GitHub
            </a>
          </Button>
        </nav>
      </div>
    </header>
  );
}

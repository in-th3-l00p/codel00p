import { LoopMark } from "@/components/site/loop-mark";

export function SiteFooter() {
  return (
    <footer className="relative z-10 border-t border-border/60">
      <div className="mx-auto flex w-full max-w-5xl flex-col items-center justify-between gap-4 px-6 py-8 sm:flex-row">
        <div className="flex items-center gap-2.5">
          <LoopMark className="size-5" />
          <span className="font-mono text-xs text-muted-foreground">
            codel00p
          </span>
        </div>
        <p className="label text-muted-foreground/60">
          MIT licensed · built in the open
        </p>
      </div>
    </footer>
  );
}

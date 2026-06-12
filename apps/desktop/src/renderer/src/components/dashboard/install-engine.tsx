import { Button } from "@/components/ui/button";
import { LoopMark } from "@/components/site/loop-mark";

const INSTALL_DOCS_URL =
  import.meta.env.RENDERER_VITE_INSTALL_DOCS_URL ??
  "https://codel00p.dev/docs/installation";

const INSTALL_COMMAND =
  "curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh";

/**
 * Shown wherever local data would appear when the `codel00p` CLI isn't installed.
 * The local engine (agents, sessions, memory on this machine) needs the binary;
 * this points the user at the installation docs.
 */
export function InstallEnginePanel() {
  function openDocs() {
    void window.codel00p?.openExternal(INSTALL_DOCS_URL);
  }

  return (
    <div className="rise mx-auto flex max-w-md flex-col items-center rounded-2xl border border-dashed border-border bg-card/30 px-8 py-10 text-center">
      <LoopMark className="size-11" />
      <p className="label mt-6 text-brand/80">Local engine</p>
      <h3 className="mt-3 text-xl font-medium tracking-tight text-foreground">
        Install the codel00p CLI
      </h3>
      <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
        The local engine powers on-machine agents, sessions, and project memory.
        It isn&apos;t detected — install the codel00p CLI to connect this machine.
      </p>

      <Button size="lg" onClick={openDocs} className="mt-7 rounded-full">
        Installation guide
        <span aria-hidden>↗</span>
      </Button>

      <div className="mt-6 w-full rounded-lg border border-border bg-background/60 px-3.5 py-2.5 text-left">
        <p className="label mb-1 text-[0.6rem] text-muted-foreground/70">
          Or run
        </p>
        <code className="block break-all font-mono text-xs text-foreground">
          {INSTALL_COMMAND}
        </code>
      </div>
    </div>
  );
}

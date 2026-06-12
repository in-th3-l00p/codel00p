import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";

/**
 * Rendered when no Clerk publishable key is configured, so the app fails loudly
 * and usefully instead of crashing inside ClerkProvider.
 */
export function MissingKeyNotice() {
  return (
    <main className="relative grid h-full place-items-center px-6 text-center">
      <GlowBackground />
      <div className="flex max-w-lg flex-col items-center">
        <LoopMark className="size-12" />
        <p className="label mt-8 text-brand/80">Configuration needed</p>
        <h1 className="mt-4 text-2xl font-medium tracking-tight text-foreground">
          Missing Clerk publishable key
        </h1>
        <p className="mt-4 text-balance text-sm leading-relaxed text-muted-foreground">
          Set{" "}
          <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.8em] text-foreground">
            RENDERER_VITE_CLERK_PUBLISHABLE_KEY
          </code>{" "}
          in{" "}
          <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.8em] text-foreground">
            apps/desktop/.env
          </code>{" "}
          then restart the dev server. Copy{" "}
          <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.8em] text-foreground">
            .env.example
          </code>{" "}
          to get started.
        </p>
      </div>
    </main>
  );
}

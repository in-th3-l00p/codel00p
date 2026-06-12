"use client";

import { useEffect, useState } from "react";

import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";

/**
 * Completes the desktop sign-in by redirecting the browser to the app's
 * localhost loopback (carrying the one-time ticket). Shown briefly before the
 * redirect, with a manual fallback link and an error state.
 */
export function DesktopHandoff({
  loopback,
  error
}: {
  loopback?: string;
  error?: string;
}) {
  const [redirected, setRedirected] = useState(false);

  useEffect(() => {
    if (!loopback) {
      return;
    }
    const timer = setTimeout(() => {
      setRedirected(true);
      window.location.replace(loopback);
    }, 350);
    return () => clearTimeout(timer);
  }, [loopback]);

  return (
    <main className="relative grid min-h-screen place-items-center px-6">
      <GlowBackground />
      <div className="rise w-full max-w-md rounded-2xl border border-border bg-card/40 p-8 text-center backdrop-blur-sm">
        <div className="mb-6 flex justify-center">
          <LoopMark className="size-10" />
        </div>
        <p className="label text-muted-foreground/70">codel00p · desktop</p>

        {error ? (
          <>
            <h1 className="mt-3 text-2xl font-medium tracking-tight text-foreground">
              Sign-in couldn&apos;t complete
            </h1>
            <p className="mt-4 rounded-lg border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-sm text-destructive">
              {error}
            </p>
            <p className="mt-4 text-xs text-muted-foreground">
              Return to the desktop app and try again.
            </p>
          </>
        ) : (
          <>
            <h1 className="mt-3 text-2xl font-medium tracking-tight text-foreground">
              You&apos;re signed in
            </h1>
            <p className="mt-4 text-sm leading-relaxed text-muted-foreground">
              {redirected
                ? "Returning you to the desktop app — you can close this tab."
                : "Handing your session back to the desktop app…"}
            </p>
            {loopback ? (
              <a
                href={loopback}
                className="mt-6 inline-flex h-9 items-center justify-center rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/80"
              >
                Return to codel00p
              </a>
            ) : null}
          </>
        )}
      </div>
    </main>
  );
}

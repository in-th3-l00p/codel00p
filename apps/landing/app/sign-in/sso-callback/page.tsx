"use client";

import { AuthenticateWithRedirectCallback } from "@clerk/nextjs";

import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";

/**
 * Where OAuth providers return after the social sign-in handshake. Clerk
 * finishes establishing the session, then forwards to the dashboard.
 */
export default function SsoCallbackPage() {
  return (
    <main className="relative grid min-h-screen place-items-center">
      <GlowBackground />
      <div className="flex flex-col items-center gap-5">
        <LoopMark className="size-10" />
        <p className="label text-muted-foreground">Finishing sign in…</p>
      </div>
      <AuthenticateWithRedirectCallback
        signInForceRedirectUrl="/dashboard"
        signUpForceRedirectUrl="/dashboard"
      />
    </main>
  );
}

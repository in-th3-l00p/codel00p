import {
  AuthenticateWithRedirectCallback,
  ClerkLoaded,
  ClerkLoading,
  SignedIn,
  SignedOut
} from "@clerk/clerk-react";

import { LoginScreen } from "./components/auth/login-screen";
import { Dashboard } from "./components/dashboard/dashboard";
import { GlowBackground } from "./components/site/glow-background";
import { LoopMark } from "./components/site/loop-mark";

/**
 * A transparent, draggable strip pinned to the top of the window. It makes the
 * frameless window movable and reserves space for the macOS traffic lights;
 * screens add matching top padding so nothing renders underneath it.
 */
function Titlebar() {
  return <div aria-hidden className="app-drag fixed inset-x-0 top-0 z-40 h-9" />;
}

/**
 * Top-level renderer surface. There is no router in the desktop shell yet, so
 * routing is path-sniffed: OAuth returns the window to `/sso-callback`, which we
 * hand to Clerk to finish the handshake. Everything else gates on auth state.
 */
export function App() {
  const isSsoCallback = window.location.pathname.startsWith("/sso-callback");

  if (isSsoCallback) {
    return (
      <>
        <Titlebar />
        <main className="relative grid h-full place-items-center">
          <GlowBackground />
          <div className="flex flex-col items-center gap-5">
            <LoopMark className="size-10" />
            <p className="label text-muted-foreground">Finishing sign in…</p>
          </div>
          <AuthenticateWithRedirectCallback
            signInForceRedirectUrl="/"
            signUpForceRedirectUrl="/"
          />
        </main>
      </>
    );
  }

  return (
    <>
      <Titlebar />
      <ClerkLoading>
        <main className="relative grid h-full place-items-center">
          <GlowBackground />
          <LoopMark className="size-10" />
        </main>
      </ClerkLoading>

      <ClerkLoaded>
        <SignedOut>
          <LoginScreen />
        </SignedOut>
        <SignedIn>
          <Dashboard />
        </SignedIn>
      </ClerkLoaded>
    </>
  );
}

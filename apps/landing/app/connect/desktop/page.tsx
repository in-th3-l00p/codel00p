import { auth } from "@clerk/nextjs/server";

import { createSignInToken } from "../../../lib/sign-in-token";
import { DesktopHandoff } from "./handoff";

/**
 * Browser-based sign-in handoff for the desktop app. The desktop opens this page
 * in the system browser with `port` (its localhost loopback) and `state`. Clerk
 * gates the page — unauthenticated users go through the hosted sign-in (where the
 * real OAuth happens) and return here. We then mint a one-time sign-in token and
 * bounce the browser back to the loopback so the app can establish a session.
 */
export default async function ConnectDesktopPage({
  searchParams
}: {
  searchParams: Promise<{ port?: string; state?: string }>;
}) {
  const { port, state } = await searchParams;
  const { userId, redirectToSignIn } = await auth();

  if (!userId) {
    return redirectToSignIn();
  }

  const portNumber = Number(port);
  const validPort =
    !!port && Number.isInteger(portNumber) && portNumber >= 1024 && portNumber <= 65535;
  if (!validPort || !state) {
    return (
      <DesktopHandoff error="This sign-in link is missing a valid callback. Start again from the desktop app." />
    );
  }

  try {
    const ticket = await createSignInToken(userId);
    const loopback = `http://127.0.0.1:${portNumber}/callback?ticket=${encodeURIComponent(
      ticket
    )}&state=${encodeURIComponent(state)}`;
    return <DesktopHandoff loopback={loopback} />;
  } catch (error) {
    return (
      <DesktopHandoff
        error={`Could not complete sign-in: ${
          error instanceof Error ? error.message : String(error)
        }`}
      />
    );
  }
}

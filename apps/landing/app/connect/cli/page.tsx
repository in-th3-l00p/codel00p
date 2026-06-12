import { auth } from "@clerk/nextjs/server";

import { CliHandoff } from "./handoff";

/**
 * Browser-based sign-in handoff for the `codel00p` CLI. The CLI opens this page
 * with `port` (its localhost loopback) and `state`. Clerk gates the page — the
 * real OAuth happens in the hosted sign-in — then we mint a longer-lived session
 * token from the "cli" JWT template and bounce the browser back to the loopback
 * so the CLI can store it. Unlike the desktop flow (which exchanges a one-time
 * ticket via Clerk's JS SDK), a Rust CLI needs an actual bearer token.
 */
export default async function ConnectCliPage({
  searchParams
}: {
  searchParams: Promise<{ port?: string; state?: string }>;
}) {
  const { port, state } = await searchParams;
  const { userId, redirectToSignIn, getToken } = await auth();

  if (!userId) {
    return redirectToSignIn();
  }

  const portNumber = Number(port);
  const validPort =
    !!port && Number.isInteger(portNumber) && portNumber >= 1024 && portNumber <= 65535;
  if (!validPort || !state) {
    return (
      <CliHandoff error="This sign-in link is missing a valid callback. Start again from the terminal." />
    );
  }

  try {
    const token = await getToken({ template: "cli" });
    if (!token) {
      throw new Error("could not mint a CLI token (is the 'cli' JWT template set up?)");
    }
    const loopback = `http://127.0.0.1:${portNumber}/callback?token=${encodeURIComponent(
      token
    )}&state=${encodeURIComponent(state)}`;
    return <CliHandoff loopback={loopback} />;
  } catch (error) {
    return (
      <CliHandoff
        error={`Could not complete sign-in: ${
          error instanceof Error ? error.message : String(error)
        }`}
      />
    );
  }
}

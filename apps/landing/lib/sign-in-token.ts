import "server-only";

/**
 * Mints a one-time Clerk sign-in token for a user via the Backend API. The
 * desktop app exchanges this token with Clerk's `ticket` strategy to establish a
 * session, so the actual OAuth happens in the system browser, not in Electron.
 */
export async function createSignInToken(
  userId: string,
  expiresInSeconds = 600
): Promise<string> {
  const secret = process.env.CLERK_SECRET_KEY;
  if (!secret) {
    throw new Error("CLERK_SECRET_KEY is not set");
  }

  const response = await fetch("https://api.clerk.com/v1/sign_in_tokens", {
    method: "POST",
    headers: {
      authorization: `Bearer ${secret}`,
      "content-type": "application/json"
    },
    body: JSON.stringify({ user_id: userId, expires_in_seconds: expiresInSeconds }),
    cache: "no-store"
  });

  if (!response.ok) {
    throw new Error(
      `sign-in token request failed (${response.status}): ${await response.text()}`
    );
  }

  const data = (await response.json()) as { token?: string };
  if (!data.token) {
    throw new Error("sign-in token response did not include a token");
  }
  return data.token;
}

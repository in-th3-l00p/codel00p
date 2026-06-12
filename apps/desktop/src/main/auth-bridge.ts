import { createServer } from "node:http";
import { randomBytes } from "node:crypto";
import { shell } from "electron";

const CONNECT_URL =
  process.env.CODEL00P_CONNECT_URL ?? "http://localhost:3000/connect/desktop";

const TIMEOUT_MS = 5 * 60 * 1000;

export type BrowserSignInResult = { ticket?: string; error?: string };

/**
 * Runs the browser-based sign-in handshake:
 *  1. start a localhost loopback server on a random port,
 *  2. open the system browser to the web handoff page (carrying the port + a
 *     CSRF `state`),
 *  3. the web app authenticates the user and redirects back to the loopback with
 *     a one-time Clerk sign-in ticket,
 *  4. resolve with that ticket so the renderer can establish a session.
 *
 * The OAuth itself happens entirely in the user's browser; Electron only listens
 * for the final loopback redirect.
 */
export function signInWithBrowser(): Promise<BrowserSignInResult> {
  return new Promise((resolve) => {
    const state = randomBytes(16).toString("hex");
    let settled = false;

    const server = createServer((request, response) => {
      const url = new URL(request.url ?? "/", "http://127.0.0.1");
      if (url.pathname !== "/callback") {
        response.writeHead(404);
        response.end();
        return;
      }

      const ticket = url.searchParams.get("ticket");
      const returnedState = url.searchParams.get("state");

      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end(resultPage(Boolean(ticket && returnedState === state)));

      if (!ticket || returnedState !== state) {
        finish({ error: "sign-in callback was invalid or out of date" });
        return;
      }
      finish({ ticket });
    });

    const timer = setTimeout(
      () => finish({ error: "browser sign-in timed out" }),
      TIMEOUT_MS
    );

    function finish(result: BrowserSignInResult) {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      server.close();
      resolve(result);
    }

    server.on("error", (error) => finish({ error: error.message }));

    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port =
        address && typeof address === "object" ? address.port : undefined;
      if (!port) {
        finish({ error: "could not start the local sign-in listener" });
        return;
      }
      const target = `${CONNECT_URL}?port=${port}&state=${state}`;
      shell.openExternal(target).catch((error) => finish({ error: String(error) }));
    });
  });
}

function resultPage(success: boolean): string {
  const heading = success ? "You&rsquo;re signed in" : "Sign-in didn&rsquo;t complete";
  const body = success
    ? "Return to the codel00p desktop app — you can close this tab."
    : "Something went wrong. Return to the desktop app and try again.";
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>codel00p</title>
<style>
  :root { color-scheme: dark; }
  body { margin:0; min-height:100vh; display:grid; place-items:center;
    background:#0c0a10; color:#f5f3f8;
    font-family:"Space Grotesk",ui-sans-serif,system-ui,-apple-system,sans-serif;
    background-image: radial-gradient(60% 40% at 50% -8%, rgba(139,124,246,.22), transparent 70%); }
  .card { text-align:center; max-width:24rem; padding:2.5rem 2rem; }
  .dot { width:12px; height:12px; border-radius:999px; background:#8b7cf6;
    box-shadow:0 0 18px #8b7cf6; margin:0 auto 1.25rem; }
  h1 { font-size:1.5rem; letter-spacing:-.02em; margin:0 0 .5rem; }
  p { color:#a59fb3; line-height:1.6; margin:0; }
</style></head>
<body><div class="card"><div class="dot"></div><h1>${heading}</h1><p>${body}</p></div></body></html>`;
}

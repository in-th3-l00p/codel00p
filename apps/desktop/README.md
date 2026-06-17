# codel00p desktop

Electron control center for supervising sessions, reviewing memory, browsing
project knowledge, and managing local provider settings.

The desktop app should be an interface over the codel00p engine and shared
protocols, not a second implementation of the agent runtime.

## Stack

- **Electron + electron-vite** — main / preload / renderer.
- **React 19 + Tailwind CSS v4** — the renderer shares the landing site's design
  system (OKLCH periwinkle brand on a near-black canvas, Space Grotesk / Pinyon
  Script / Geist Mono, the `GlowBackground` + `LoopMark` brand visuals).
- **shadcn-style components** — `components/ui/*` (Button, Input) built on
  `radix-ui` + `class-variance-authority`, aliased under `@/*`.
- **Clerk** (`@clerk/clerk-react`) — authentication.

## Authentication

The login screen (`components/auth`) is a branded split-screen with two paths:

- **Continue in browser** (primary) — the secure desktop OAuth pattern. The
  Electron main process opens a localhost loopback server and launches the
  **system browser** to the cloud web app's `/connect/desktop` handoff page. The
  user signs in there (full OAuth/GitHub/Google/email via Clerk's hosted portal),
  the web app mints a one-time Clerk **sign-in token**, and redirects back to the
  loopback. The renderer exchanges that ticket with Clerk's `ticket` strategy and
  activates the session. See `main/auth-bridge.ts`,
  `apps/landing/app/connect/desktop`, and set `CODEL00P_CONNECT_URL` to point at the
  web app (defaults to the dev server `http://localhost:3000`).
- **Email code** (fallback) — a one-time code entered in-window via `useSignIn`,
  for when the browser flow isn't available.

The browser flow is preferred because the OAuth never runs inside Electron (no
`file://`/origin caveats, and credentials stay in the user's real browser).

### Setup

1. Create an application at https://dashboard.clerk.com and enable **Email code**,
   **GitHub**, and **Google** as sign-in options.
2. `cp .env.example .env` and set `RENDERER_VITE_CLERK_PUBLISHABLE_KEY` to the
   app's publishable key (`pk_test_…`).
3. `pnpm dev:desktop` from the repo root.

Without a key the app renders a configuration notice instead of crashing.

> **Electron note:** OAuth redirects rely on the renderer being served over an
> http origin, which the `electron-vite dev` server provides. Production
> (`file://`) packaging will need a custom protocol or local server for the
> OAuth round-trip; the email-code flow works without redirects.

## Dashboard

After sign-in the app shows a read-only **dashboard** (`components/dashboard`):
a sidebar that switches what to visualize (Overview · Organizations · Projects ·
Agents) and a top bar that filters by **source** (All / Cloud / Local), switches
the active organization (Clerk `OrganizationSwitcher`), and refreshes. It only
shows navigation and data.

Data sources:

- **Cloud** — organizations from the user's Clerk memberships, and projects from
  the `codel00p-cloud` service via `@codel00p/sdk` (active-org scoped, using the
  Clerk session token). Set `RENDERER_VITE_CODEL00P_API_URL` (default
  `http://localhost:8787`). Enable **Organizations** in the Clerk instance
  (`clerk enable orgs`) so the switcher works.
- **Local** — agent sessions from the `codel00p` CLI, fetched through an Electron
  IPC bridge (`main/local-engine.ts` → `local:sessions`). The binary is resolved
  from `CODEL00P_BIN`, then the workspace build, then `PATH`. If the CLI or its
  store isn't available the dashboard shows a clear "not connected" state.

Cloud agent runs are not exposed by the service yet, so the Agents view shows
local sessions for now.

## Commands

```bash
pnpm dev:desktop      # run the app (electron-vite dev)
pnpm --filter @codel00p/desktop build      # build main/preload/renderer
pnpm --filter @codel00p/desktop dist       # build a local packaged app
pnpm --filter @codel00p/desktop typecheck  # tsc --noEmit
```

## Releases

Desktop installers are published by the same `v*` tag workflow that ships the
CLI. `.github/workflows/release.yml` builds Electron Builder installers for
macOS, Linux, and Windows, then normalizes them to stable GitHub Release asset
names:

- `codel00p-desktop-aarch64-apple-darwin.dmg`
- `codel00p-desktop-x86_64-apple-darwin.dmg`
- `codel00p-desktop-x86_64-unknown-linux-gnu.AppImage`
- `codel00p-desktop-aarch64-unknown-linux-gnu.AppImage`
- `codel00p-desktop-x86_64-pc-windows-msvc.exe`

Each asset gets a `.sha256` sidecar. When desktop changes ship, bump
`apps/desktop/package.json`, commit, and push the `vX.Y.Z` tag used for the
release.

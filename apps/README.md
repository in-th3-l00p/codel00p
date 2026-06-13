# Applications

Deployable and user-facing codel00p applications.

- `landing`: public Next.js site — marketing, documentation, and the
  authenticated cloud control surface (sign-in + organization dashboard).
  Deploys to Vercel.
- `desktop`: Electron control center.

The cloud control-plane API is a Rust service (`core/crates/codel00p-cloud`);
the web and desktop apps talk to it through `@codel00p/sdk`.

Apps may depend on shared packages under `packages/`, but shared product logic
should not live in app directories.

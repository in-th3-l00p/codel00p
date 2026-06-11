# @codel00p/protocol-ts

TypeScript protocol types shared by web, desktop, cloud, and SDK packages.

Long term, this package should be generated from or checked against
`core/crates/codel00p-protocol`.

It currently exports shared memory/session protocol types plus the built-in
provider policy preset catalog used by cloud, desktop, and SDK control-plane
surfaces. The preset IDs match `ProviderPolicy::presets()` in
`codel00p-providers`.

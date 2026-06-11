# @codel00p/sdk

TypeScript client SDK for codel00p apps.

This package should wrap cloud APIs, desktop bridge contracts, and shared
protocol types without duplicating product logic from the Rust engine.

Provider policy preset metadata is re-exported from `@codel00p/protocol-ts` so
cloud and desktop configuration surfaces can present stable built-in policy
defaults by ID.

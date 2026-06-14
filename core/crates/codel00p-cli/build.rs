//! Stamps the binary with its release version.
//!
//! Releases build with `CODEL00P_RELEASE_VERSION` set from the pushed `v*` tag (see
//! `.github/workflows/release.yml`), so the shipped binary reports the exact tag it
//! came from — which the self-updater compares against the latest GitHub release.
//! Local builds fall back to the crate's `Cargo.toml` version.

use std::env;

fn main() {
    let version = env::var("CODEL00P_RELEASE_VERSION")
        .ok()
        .map(|value| value.trim().trim_start_matches('v').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string()));

    println!("cargo:rustc-env=CODEL00P_VERSION={version}");
    println!("cargo:rerun-if-env-changed=CODEL00P_RELEASE_VERSION");
}

//! Verifies the binary-resolution mechanism works from this separate crate.
//!
//! `assert_cmd::cargo_bin("codel00p")` does NOT work here: `CARGO_BIN_EXE_codel00p`
//! is only set for the crate that owns the binary target. We resolve from the
//! workspace target dir instead (see [`codel00p_e2e::codel00p_binary`]).

use std::process::Command;

use codel00p_e2e::codel00p_binary;

#[test]
fn resolves_and_runs_codel00p_cross_crate() {
    let bin = codel00p_binary();
    assert!(
        bin.exists(),
        "codel00p binary not found at {bin:?} — is the codel00p-cli dev-dependency building it?"
    );
    let output = Command::new(&bin)
        .arg("--help")
        .output()
        .expect("run codel00p --help");
    assert!(
        output.status.success(),
        "codel00p --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

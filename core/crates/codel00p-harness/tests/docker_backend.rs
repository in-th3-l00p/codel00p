//! Live Docker backend integration tests.
//!
//! These actually invoke `docker` and run containers, so they are gated two
//! ways and never run by default:
//!
//!   1. Every test is `#[ignore]`, so a plain `cargo test` skips them.
//!   2. Even when run with `--ignored`, each test first checks the
//!      `CODEL00P_DOCKER_TESTS=1` opt-in env var AND that the Docker daemon is
//!      reachable; if either is missing it prints a skip note and returns
//!      cleanly (mirroring the `CODEL00P_INTEGRATION_TESTS` convention in
//!      `codel00p-providers/tests/support`).
//!
//! To run them locally (requires Docker running and the `alpine` image
//! pullable):
//!
//! ```sh
//! CODEL00P_DOCKER_TESTS=1 cargo test -p codel00p-harness --test docker_backend -- --ignored
//! ```

use std::{
    process::{Command, Stdio},
    time::Duration,
};

use codel00p_harness::{CommandSpec, DockerBackend, DockerConfig, OutputLimits, TerminalBackend};

/// True when the opt-in env var is set and the Docker daemon answers.
fn docker_tests_enabled() -> bool {
    let opted_in = matches!(
        std::env::var("CODEL00P_DOCKER_TESTS")
            .unwrap_or_default()
            .trim(),
        "1" | "true" | "yes" | "on"
    );
    if !opted_in {
        return false;
    }
    Command::new("docker")
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

macro_rules! require_docker {
    () => {
        if !docker_tests_enabled() {
            eprintln!(
                "skipping live Docker test: set CODEL00P_DOCKER_TESTS=1 and ensure the Docker daemon is reachable"
            );
            return;
        }
    };
}

fn limits(timeout_ms: u64, max_bytes: usize) -> OutputLimits {
    OutputLimits {
        timeout: Duration::from_millis(timeout_ms),
        max_output_bytes: max_bytes,
    }
}

fn backend(workspace: &std::path::Path) -> DockerBackend {
    DockerBackend::new(workspace, DockerConfig::new("alpine"))
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn foreground_success_captures_stdout_and_stderr() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    let outcome = backend(ws.path())
        .run_foreground(
            &CommandSpec::new(
                "sh",
                vec!["-c".into(), "printf hi; printf err 1>&2".into()],
                ws.path().to_path_buf(),
            ),
            limits(30_000, 16_384),
        )
        .unwrap();
    assert!(outcome.success, "stderr: {}", outcome.stderr);
    assert!(!outcome.timed_out);
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(outcome.stdout, "hi");
    assert_eq!(outcome.stderr, "err");
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn foreground_nonzero_exit_is_proxied() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    let outcome = backend(ws.path())
        .run_foreground(
            &CommandSpec::new(
                "sh",
                vec!["-c".into(), "exit 7".into()],
                ws.path().to_path_buf(),
            ),
            limits(30_000, 16_384),
        )
        .unwrap();
    assert!(!outcome.success);
    assert!(!outcome.timed_out);
    assert_eq!(outcome.exit_code, Some(7));
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn foreground_output_is_capped() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    let outcome = backend(ws.path())
        .run_foreground(
            &CommandSpec::new(
                "sh",
                vec!["-c".into(), "printf aaaaaaaaaa".into()],
                ws.path().to_path_buf(),
            ),
            limits(30_000, 4),
        )
        .unwrap();
    assert_eq!(outcome.stdout, "aaaa");
    assert!(outcome.stdout_truncated);
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn foreground_timeout_kills_container_with_no_orphan() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    // Use the network-less default; a sleeping container that outlives the
    // timeout must be killed and (via --rm) removed.
    let outcome = backend(ws.path())
        .run_foreground(
            &CommandSpec::new(
                "sh",
                vec!["-c".into(), "sleep 60".into()],
                ws.path().to_path_buf(),
            ),
            limits(1_500, 16_384),
        )
        .unwrap();
    assert!(outcome.timed_out);
    assert!(!outcome.success);
    assert_eq!(outcome.exit_code, None);

    // Give Docker a beat to finish removing the killed container, then assert
    // no codel00p container from this process is left behind.
    std::thread::sleep(Duration::from_millis(1_000));
    let listed = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=codel00p-{}-", std::process::id()),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .unwrap();
    let names = String::from_utf8_lossy(&listed.stdout);
    assert!(
        names.trim().is_empty(),
        "expected no orphaned containers, found: {names}"
    );
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn background_spawn_output_then_kill_stops_container() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    let backend = backend(ws.path());

    // Short-lived: drain stdout, confirm exit 0.
    let mut handle = backend
        .spawn_background(&CommandSpec::new(
            "sh",
            vec!["-c".into(), "printf done".into()],
            ws.path().to_path_buf(),
        ))
        .unwrap();
    let mut stdout = handle.take_stdout().expect("stdout pipe");
    let mut buf = String::new();
    use std::io::Read;
    stdout.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "done");
    assert_eq!(handle.wait().unwrap(), Some(0));

    // Long-lived: kill it and confirm the container is gone (kill closes the
    // pipes, --rm removes it).
    let mut handle = backend
        .spawn_background(&CommandSpec::new(
            "sh",
            vec!["-c".into(), "sleep 60".into()],
            ws.path().to_path_buf(),
        ))
        .unwrap();
    // Drain stdout in a thread so the read EOFs after kill (mirrors the store).
    let mut out = handle.take_stdout().expect("stdout");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
    });
    // Let the container start.
    std::thread::sleep(Duration::from_millis(800));
    assert!(handle.try_wait().unwrap().is_none());
    handle.kill().unwrap();
    let _ = handle.wait();
    reader.join().unwrap();

    std::thread::sleep(Duration::from_millis(1_000));
    let listed = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=codel00p-{}-", std::process::id()),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .unwrap();
    let names = String::from_utf8_lossy(&listed.stdout);
    assert!(
        names.trim().is_empty(),
        "expected killed background container to be removed, found: {names}"
    );
}

#[test]
#[ignore = "requires CODEL00P_DOCKER_TESTS=1 and a running Docker daemon"]
fn file_created_in_container_appears_on_host_with_host_ownership() {
    require_docker!();
    let ws = tempfile::tempdir().unwrap();
    let backend = DockerBackend::new(ws.path(), DockerConfig::new("alpine"));
    let outcome = backend
        .run_foreground(
            &CommandSpec::new(
                "sh",
                vec!["-c".into(), "printf hello > created.txt".into()],
                ws.path().to_path_buf(),
            ),
            limits(30_000, 16_384),
        )
        .unwrap();
    assert!(outcome.success, "stderr: {}", outcome.stderr);

    let host_path = ws.path().join("created.txt");
    assert!(host_path.exists(), "file should persist on the host");
    assert_eq!(std::fs::read_to_string(&host_path).unwrap(), "hello");

    // With map_host_user (the default) the file is owned by the host uid, so
    // the host process can read AND remove it without permission errors.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(&host_path).unwrap();
        // SAFETY: getuid has no preconditions.
        let host_uid = unsafe { libc::getuid() };
        assert_eq!(
            meta.uid(),
            host_uid,
            "file should be owned by the host user"
        );
    }
    std::fs::remove_file(&host_path).expect("host should be able to remove the file");
}

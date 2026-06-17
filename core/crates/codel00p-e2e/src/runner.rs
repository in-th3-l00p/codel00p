//! The isolated-world test runner that spawns the real `codel00p` binary.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;

use crate::{MockProvider, RunResult};

/// Resolves the path to the compiled `codel00p` binary, building it if absent.
///
/// `assert_cmd::cargo_bin("codel00p")` cannot find the binary from this crate:
/// `CARGO_BIN_EXE_codel00p` is only injected for the crate that declares the
/// binary target (`codel00p-cli`), and a normal dev-dependency on a binary-only
/// crate is *ignored* by Cargo ("missing a lib target"). So we resolve the
/// binary from the workspace target directory relative to this crate's manifest,
/// and — to make `cargo test -p codel00p-e2e` self-contained locally — build it
/// on demand (once) if it is missing. CI additionally runs
/// `cargo build -p codel00p-cli` first, so the build is a fast no-op there.
///
/// Honors `CARGO_TARGET_DIR` if set; otherwise uses `<workspace>/target`. The
/// build profile is inferred from this test binary's own path (`debug` vs
/// `release`), falling back to `debug`.
#[must_use]
pub fn codel00p_binary() -> PathBuf {
    use std::sync::OnceLock;
    static RESOLVED: OnceLock<PathBuf> = OnceLock::new();
    RESOLVED.get_or_init(resolve_or_build_binary).clone()
}

fn resolve_or_build_binary() -> PathBuf {
    let exe = if cfg!(windows) {
        "codel00p.exe"
    } else {
        "codel00p"
    };

    // <manifest>/crates/codel00p-e2e -> workspace root is two levels up.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&manifest_dir)
        .to_path_buf();

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));

    let profile = current_profile();

    if let Some(found) = find_existing(&target_dir, exe, &profile) {
        return found;
    }

    // Not built yet: build it once for the active profile.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(cargo);
    build
        .current_dir(&workspace_root)
        .args(["build", "-p", "codel00p-cli", "--bin", "codel00p"]);
    if profile == "release" {
        build.arg("--release");
    }
    let status = build.status().expect("build codel00p binary");
    assert!(status.success(), "`cargo build -p codel00p-cli` failed");

    find_existing(&target_dir, exe, &profile)
        .unwrap_or_else(|| panic!("codel00p binary missing after build under {target_dir:?}"))
}

fn find_existing(target_dir: &Path, exe: &str, profile: &str) -> Option<PathBuf> {
    let primary = target_dir.join(profile).join(exe);
    if primary.exists() {
        return Some(primary);
    }
    // Fall back to the other common profile dir.
    for alt in ["debug", "release"] {
        let alt_path = target_dir.join(alt).join(exe);
        if alt_path.exists() {
            return Some(alt_path);
        }
    }
    None
}

/// Infers the active Cargo profile directory from the test executable's path.
fn current_profile() -> String {
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
                if name == "release" {
                    return "release".to_string();
                }
                if name == "debug" {
                    return "debug".to_string();
                }
            }
        }
    }
    "debug".to_string()
}

/// Builds and runs the real binary inside a hermetic, isolated world.
///
/// One runner owns one `CODEL00P_HOME` and one workspace tempdir. Reusing the
/// same runner for a second [`CodelRunner::run`] (e.g. `agent continue`) reuses
/// the *same* home and workspace, which is exactly what replay scenarios need.
pub struct CodelRunner {
    home: TempDir,
    workspace: TempDir,
    extra_env: BTreeMap<String, String>,
    base_url: Option<String>,
    permission_mode: String,
}

impl Default for CodelRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CodelRunner {
    /// Creates a runner with a fresh `CODEL00P_HOME` and workspace tempdir.
    #[must_use]
    pub fn new() -> Self {
        Self {
            home: TempDir::new().expect("create CODEL00P_HOME tempdir"),
            workspace: TempDir::new().expect("create workspace tempdir"),
            extra_env: BTreeMap::new(),
            base_url: None,
            // Default: auto-approve every tool so existing scenarios stay green.
            permission_mode: "allow".to_string(),
        }
    }

    /// Overrides the `--permission-mode` the runner appends to `agent`
    /// subcommands (default `allow`). Use `"ask"` or `"deny"` to exercise the
    /// permission policy. Because the runner appends its provider flags *after*
    /// the caller's args, setting the mode here (rather than passing
    /// `--permission-mode` in `run`) is the only way to make it take effect — a
    /// later flag wins in the CLI's last-write parser.
    #[must_use]
    pub fn permission_mode(mut self, mode: impl Into<String>) -> Self {
        self.permission_mode = mode.into();
        self
    }

    /// Initializes the workspace as a git repo (`git init -b main`, identity, and
    /// an initial commit). Always uses `-b main` because CI git defaults to
    /// `master`.
    #[must_use]
    pub fn git_init(self) -> Self {
        let ws = self.workspace.path();
        run_git(ws, &["init", "-b", "main"]);
        run_git(ws, &["config", "user.name", "codel00p-e2e"]);
        run_git(ws, &["config", "user.email", "e2e@codel00p.test"]);
        // An initial commit so `git log` and committing have a baseline.
        std::fs::write(ws.join(".gitkeep"), "").expect("seed .gitkeep");
        run_git(ws, &["add", "."]);
        run_git(ws, &["commit", "-m", "chore: initial commit"]);
        self
    }

    /// Seeds a file in the workspace (creating parent directories as needed).
    #[must_use]
    pub fn workspace_file(self, rel_path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Self {
        let path = self.workspace.path().join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create workspace parent dirs");
        }
        std::fs::write(&path, contents).expect("seed workspace file");
        self
    }

    /// Attaches a scripted mock provider; the runner will pass `--provider
    /// custom --model test-model --base-url <url> --json-events
    /// --permission-mode <mode>` automatically (the mode defaults to `allow`;
    /// override it with [`CodelRunner::permission_mode`]).
    #[must_use]
    pub fn with_provider(mut self, provider: &MockProvider) -> Self {
        self.base_url = Some(provider.base_url());
        self
    }

    /// Sets an extra environment variable for the spawned binary.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.insert(key.into(), value.into());
        self
    }

    /// The workspace directory (for seeding/asserting side effects).
    #[must_use]
    pub fn workspace_path(&self) -> &Path {
        self.workspace.path()
    }

    /// The `CODEL00P_HOME` directory.
    #[must_use]
    pub fn home_path(&self) -> &Path {
        self.home.path()
    }

    /// The path to this run's `memory.sqlite` database.
    #[must_use]
    pub fn memory_db(&self) -> PathBuf {
        self.home.path().join("memory.sqlite")
    }

    /// Spawns the binary with the given subcommand args, injecting the isolated
    /// home, memory-db, org/project flags, provider key, and (when a mock
    /// provider is attached) the provider/model/base-url/json-events/permission
    /// flags. Returns a structured [`RunResult`].
    #[must_use]
    pub fn run(&self, args: &[&str]) -> RunResult {
        let db_path = self.memory_db();
        let mut command = Command::new(codel00p_binary());
        command
            // Run inside the isolated workspace tempdir so project-config
            // discovery (which walks up from the cwd looking for
            // `.codel00p/config.toml`) cannot reach the developer's real
            // `~/.codel00p` and leak settings into a "hermetic" run. With
            // `CODEL00P_HOME` pointed at a tempdir, the user-config-dir
            // exclusion no longer covers `~/.codel00p`, so isolating the cwd is
            // what actually keeps config-sensitive commands hermetic.
            .current_dir(self.workspace.path())
            .env("CODEL00P_HOME", self.home.path())
            .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
            .arg("--memory-db")
            .arg(&db_path)
            .arg("--organization-id")
            .arg("org-1")
            .arg("--project-id")
            .arg("project-1")
            .arg("--project-name")
            .arg("codel00p");

        for (key, value) in &self.extra_env {
            command.env(key, value);
        }

        command.args(args);

        // Provider wiring is appended after the user args so it lands on the
        // agent subcommand's flag list — but only for `agent` subcommands, since
        // commands like `session list` reject these flags.
        let is_agent_command = args.first() == Some(&"agent");
        if is_agent_command && let Some(base_url) = &self.base_url {
            command
                .arg("--workspace")
                .arg(self.workspace.path())
                .arg("--provider")
                .arg("custom")
                .arg("--model")
                .arg("test-model")
                .arg("--base-url")
                .arg(base_url)
                .arg("--json-events")
                .arg("--permission-mode")
                .arg(&self.permission_mode);
        }

        let output = command.output().expect("spawn codel00p binary");
        RunResult::new(output)
    }

    /// Spawns the binary with *exactly* the given args — no injected global flags
    /// (`--memory-db`, `--organization-id`, …) and no provider wiring — inside the
    /// isolated `CODEL00P_HOME`.
    ///
    /// Required for surfaces that parse the raw argv positionally *before* the
    /// global-flag parser runs: `version` / `--version` are detected via
    /// `args.first()`, and `--help` / `<command> --help` match the exact argv
    /// slice. The runner's standard [`CodelRunner::run`] prepends the four global
    /// flags, which would shift those positions and make `version`/`help` fall
    /// through to "unknown command". Use this for those black-box surfaces; use
    /// [`CodelRunner::run`] for everything that goes through the global-flag
    /// parser (config, providers, skills, cron, cloud, update, …).
    #[must_use]
    pub fn run_plain(&self, args: &[&str]) -> RunResult {
        let mut command = Command::new(codel00p_binary());
        command
            // Same isolation rationale as `run`: keep project-config discovery
            // off the developer's real `~/.codel00p`.
            .current_dir(self.workspace.path())
            .env("CODEL00P_HOME", self.home.path())
            .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token");
        for (key, value) in &self.extra_env {
            command.env(key, value);
        }
        command.args(args);
        let output = command.output().expect("spawn codel00p binary");
        RunResult::new(output)
    }
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run `git {}`: {error}", args.join(" ")));
    assert!(
        status.status.success(),
        "`git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&status.stderr)
    );
}

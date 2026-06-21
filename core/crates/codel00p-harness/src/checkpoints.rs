//! Shadow-git checkpoints: per-step workspace snapshots the agent can roll back
//! to, so aggressive autonomy is safely undoable.
//!
//! # Mechanism
//!
//! A checkpoint is a commit in a **shadow git directory** that lives *outside*
//! the workspace (under codel00p's home/state, keyed by a hash of the workspace
//! path). Every git operation runs with an explicit
//! `--git-dir=<shadow> --work-tree=<workspace>` pair, so the shadow repo
//! snapshots the real working tree — including untracked files — **without ever
//! creating or touching a `.git` inside the workspace**. If the workspace is
//! itself a real git repo, its `.git` is never read or written by these
//! operations: the separate git-dir + work-tree is the isolation boundary. The
//! shadow repo's `info/exclude` lists `.git/`, so any `.git` directory in the
//! work-tree (the real repo, or nested ones) is **never staged by `add -A` and
//! never removed by `clean -fd` on restore** — restoring a checkpoint can roll
//! back the working tree but can never revert or delete real git history. We
//! also never run `git` *inside* the workspace's own repo, so its
//! HEAD/branches/index/reflog are untouched.
//!
//! Restoring (`restore(id, mode)`) checks the snapshot's tree out into the
//! work-tree via the shadow git-dir, affecting only the workspace's files on
//! disk — never the user's real repo history.

use std::path::{Path, PathBuf};
use std::process::Command;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

/// Identity used for shadow commits so snapshots do not depend on the user's
/// ambient git identity (which may be unset in CI / sandboxes).
const SHADOW_USER_NAME: &str = "codel00p-checkpoints";
const SHADOW_USER_EMAIL: &str = "checkpoints@codel00p.local";

/// A checkpoint store backed by a shadow git directory kept outside the
/// workspace. One store maps to one workspace; the shadow git-dir is derived
/// from `base_dir` plus a stable hash of the workspace root.
///
/// All git invocations go through [`CheckpointStore::git`], which always passes
/// the `--git-dir`/`--work-tree` pair so the shadow repo operates on the real
/// working tree without depending on the cwd or any in-workspace `.git`.
#[derive(Clone, Debug)]
pub struct CheckpointStore {
    shadow_git_dir: PathBuf,
    workspace_root: PathBuf,
}

/// A single checkpoint: the shadow commit sha, its label, and creation time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: String,
    pub label: Option<String>,
    /// ISO-8601 / RFC-2822-ish committer date as reported by git, if available.
    pub created_at: Option<String>,
}

impl Checkpoint {
    fn to_json(&self) -> Value {
        let mut value = json!({ "id": self.id });
        if let Some(label) = &self.label {
            value["label"] = json!(label);
        }
        if let Some(created_at) = &self.created_at {
            value["created_at"] = json!(created_at);
        }
        value
    }
}

impl CheckpointStore {
    /// Build a store for `workspace_root`, placing its shadow git-dir under
    /// `base_dir/checkpoints/<workspace-hash>/.git`. The directory is created
    /// lazily on first snapshot via [`CheckpointStore::ensure_initialized`].
    pub fn new(base_dir: impl AsRef<Path>, workspace_root: impl AsRef<Path>) -> Self {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let hash = workspace_hash(&workspace_root);
        let shadow_git_dir = base_dir
            .as_ref()
            .join("checkpoints")
            .join(hash)
            .join(".git");
        Self {
            shadow_git_dir,
            workspace_root,
        }
    }

    /// The shadow git directory this store uses (outside the workspace).
    pub fn shadow_git_dir(&self) -> &Path {
        &self.shadow_git_dir
    }

    /// Snapshot the current workspace working tree (including untracked files)
    /// and return the resulting checkpoint.
    pub fn snapshot(&self, label: Option<&str>) -> Result<Checkpoint, HarnessError> {
        self.ensure_initialized()?;
        // Stage everything in the work-tree, then commit. `--allow-empty` so a
        // snapshot with no changes since the last one still yields a distinct
        // checkpoint id the agent can return to; `--no-verify` so the user's
        // hooks never run against the shadow repo.
        self.git(&["add", "-A"])?;
        let message = label.unwrap_or("checkpoint");
        self.git(&["commit", "--allow-empty", "--no-verify", "-m", message])?;
        let id = self.git(&["rev-parse", "HEAD"])?.trim().to_string();
        let created_at = self
            .git(&["show", "-s", "--format=%cI", &id])
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(Checkpoint {
            id,
            label: label.map(str::to_string),
            created_at,
        })
    }

    /// List checkpoints, newest first (matching `git log` order).
    pub fn list(&self) -> Result<Vec<Checkpoint>, HarnessError> {
        if !self.shadow_git_dir.exists() {
            return Ok(Vec::new());
        }
        // No commits yet (freshly initialized) → empty list rather than error.
        if self.git(&["rev-parse", "--verify", "HEAD"]).is_err() {
            return Ok(Vec::new());
        }
        // %x1f (unit separator) splits the fields; the subject is the label.
        let output = self.git(&["log", "--format=%H%x1f%cI%x1f%s"])?;
        let checkpoints = output
            .lines()
            .filter_map(parse_log_line)
            .collect::<Vec<_>>();
        Ok(checkpoints)
    }

    /// Restore the workspace working tree to checkpoint `id`.
    ///
    /// Only `mode = "files"` (the default) is supported: it checks the snapshot's
    /// tree out into the work-tree and removes files that were not part of the
    /// snapshot, so the workspace matches the checkpoint exactly. Returns the
    /// number of paths whose working-tree state differed from the checkpoint
    /// before restoring (i.e. how many files the restore touched), when cheaply
    /// available.
    pub fn restore(&self, id: &str, mode: RestoreMode) -> Result<RestoreOutcome, HarnessError> {
        self.ensure_initialized()?;
        // Validate the id is a real commit in the shadow repo before mutating.
        self.git(&["rev-parse", "--verify", &format!("{id}^{{commit}}")])
            .map_err(|_| {
                tool_failed("restore_checkpoint", format!("unknown checkpoint id: {id}"))
            })?;

        // How many paths differ from the target snapshot right now — this is the
        // count of files the restore will change. Computed before the mutation.
        let files_restored = self
            .git(&["diff", "--name-only", id])
            .ok()
            .map(|diff| diff.lines().filter(|line| !line.is_empty()).count());

        match mode {
            RestoreMode::Files => {
                // Move the shadow HEAD to the snapshot and force the work-tree to
                // match it, including deleting files added after the snapshot.
                // `reset --hard` against the shadow git-dir rewrites only the
                // workspace working tree (and the shadow index); it never touches
                // the user's real repo because that repo is never the git-dir.
                self.git(&["reset", "--hard", id])?;
                self.git(&["clean", "-fd"])?;
            }
        }

        Ok(RestoreOutcome {
            id: id.to_string(),
            files_restored,
        })
    }

    /// Create the shadow git directory and configure it if it does not exist.
    fn ensure_initialized(&self) -> Result<(), HarnessError> {
        if self.shadow_git_dir.join("HEAD").exists() {
            return Ok(());
        }
        if let Some(parent) = self.shadow_git_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                tool_failed("checkpoints", format!("creating shadow store: {error}"))
            })?;
        }
        // `git init` a *bare-style separate* git-dir for the work-tree. We pass
        // the explicit git-dir/work-tree so no `.git` is created in the
        // workspace.
        self.git(&["init", "-q"])?;
        // SAFETY-CRITICAL: never let the shadow repo capture any `.git`
        // directory in the work-tree. Without this, `add -A` would stage the
        // user's real `.git` *as files*, and a later `restore` (`reset --hard`
        // + `clean -fd`) could revert/delete real repo internals created after
        // the snapshot — corrupting the user's actual history. Excluding `.git/`
        // means it is never staged AND `clean -fd` never removes it (ignored
        // files are preserved without `-x`). The pattern (no leading slash,
        // trailing slash) matches the top-level real repo and any nested `.git`.
        let exclude_path = self.shadow_git_dir.join("info").join("exclude");
        if let Some(parent) = exclude_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                tool_failed("checkpoints", format!("creating shadow exclude: {error}"))
            })?;
        }
        std::fs::write(&exclude_path, ".git/\n").map_err(|error| {
            tool_failed("checkpoints", format!("writing shadow exclude: {error}"))
        })?;
        Ok(())
    }

    /// Run a git command against the shadow git-dir + workspace work-tree.
    ///
    /// Always injects the identity / gpg-sign config inline (`-c …`) so commits
    /// do not depend on the ambient git environment, and always passes the
    /// `--git-dir`/`--work-tree` pair so the operation targets the shadow repo
    /// over the real working tree — never any `.git` inside the workspace.
    fn git(&self, args: &[&str]) -> Result<String, HarnessError> {
        let git_dir = self.shadow_git_dir.to_string_lossy().to_string();
        let work_tree = self.workspace_root.to_string_lossy().to_string();
        let mut command = Command::new("git");
        command
            .arg(format!("--git-dir={git_dir}"))
            .arg(format!("--work-tree={work_tree}"))
            .args([
                "-c",
                "commit.gpgSign=false",
                "-c",
                &format!("user.name={SHADOW_USER_NAME}"),
                "-c",
                &format!("user.email={SHADOW_USER_EMAIL}"),
            ])
            .args(args)
            // Run from the workspace so relative work-tree resolution and
            // `clean`/`add` see the intended files.
            .current_dir(&self.workspace_root);

        let output = command
            .output()
            .map_err(|error| tool_failed("checkpoints", error.to_string()))?;
        if !output.status.success() {
            return Err(tool_failed(
                "checkpoints",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Supported restore strategies. Only `Files` exists today; the enum leaves room
/// for future modes (e.g. a task-level "restore files + conversation").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreMode {
    /// Restore the workspace working tree to the snapshot.
    Files,
}

impl RestoreMode {
    fn parse(value: Option<&str>) -> Result<Self, HarnessError> {
        match value {
            None | Some("files") => Ok(RestoreMode::Files),
            Some(other) => Err(HarnessError::InvalidToolInput {
                name: "restore_checkpoint".to_string(),
                message: format!("`mode` must be \"files\", got `{other}`"),
            }),
        }
    }
}

/// What a restore did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub id: String,
    pub files_restored: Option<usize>,
}

fn parse_log_line(line: &str) -> Option<Checkpoint> {
    let mut parts = line.split('\x1f');
    let id = parts.next()?.to_string();
    let created_at = parts.next().map(str::to_string).filter(|v| !v.is_empty());
    let subject = parts.next().unwrap_or("");
    let label = if subject.is_empty() || subject == "checkpoint" {
        None
    } else {
        Some(subject.to_string())
    };
    Some(Checkpoint {
        id,
        label,
        created_at,
    })
}

/// A filesystem-safe, stable hash of the workspace root path, used to key the
/// shadow store so each workspace gets its own snapshot history.
fn workspace_hash(workspace_root: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    workspace_root.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}

/// Build a [`CheckpointStore`] for a workspace, given the shadow base dir.
fn store_for(base_dir: &Path, workspace: &Workspace) -> CheckpointStore {
    CheckpointStore::new(base_dir, workspace.root())
}

/// The three checkpoint tools (`create_checkpoint`, `list_checkpoints`,
/// `restore_checkpoint`) as a registry, all backed by the shadow store under
/// `base_dir`. Folded into the `git`/`all` tool sets by the CLI, which supplies
/// the real codel00p home dir as `base_dir`.
pub fn checkpoint_tools(base_dir: impl AsRef<Path>) -> crate::tool_registry::ToolRegistry {
    let base_dir = base_dir.as_ref();
    crate::tool_registry::ToolRegistry::new()
        .with_tool(CreateCheckpointTool::new(base_dir))
        .with_tool(ListCheckpointsTool::new(base_dir))
        .with_tool(RestoreCheckpointTool::new(base_dir))
}

/// `create_checkpoint` — snapshot the workspace.
///
/// Scope: [`PermissionScope::ReadOnly`]. Creating a checkpoint reads the
/// workspace and writes only to the shadow store *outside* the workspace; it does
/// not mutate any workspace file, so it is treated as a read-only-to-the-user
/// operation (mirrors `git_status`/`git_diff`, which also read the tree).
pub struct CreateCheckpointTool {
    base_dir: PathBuf,
}

impl CreateCheckpointTool {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for CreateCheckpointTool {
    fn name(&self) -> &str {
        "create_checkpoint"
    }

    fn description(&self) -> &str {
        "Snapshot the entire workspace (including untracked files) to a shadow \
         git store kept outside the workspace, returning a checkpoint id you can \
         later restore to with `restore_checkpoint`. Does not modify the \
         workspace or the user's real git repository. Use before an aggressive or \
         risky edit so the change is undoable."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "label": { "type": "string" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let label = optional_string(&input, "label");
        let store = store_for(&self.base_dir, workspace);
        let checkpoint = store.snapshot(label)?;
        Ok(ToolResult::json(checkpoint.to_json()))
    }
}

/// `list_checkpoints` — list snapshots newest-first. Read-only.
pub struct ListCheckpointsTool {
    base_dir: PathBuf,
}

impl ListCheckpointsTool {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for ListCheckpointsTool {
    fn name(&self) -> &str {
        "list_checkpoints"
    }

    fn description(&self) -> &str {
        "List workspace checkpoints (shadow-git snapshots), newest first, each \
         with its id, optional label, and creation time. Read-only."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let store = store_for(&self.base_dir, workspace);
        let checkpoints = store.list()?;
        let checkpoints: Vec<Value> = checkpoints.iter().map(Checkpoint::to_json).collect();
        Ok(ToolResult::json(json!({ "checkpoints": checkpoints })))
    }
}

/// `restore_checkpoint` — roll the workspace working tree back to a snapshot.
///
/// Scope: [`PermissionScope::WorkspaceWrite`] — this overwrites workspace files.
pub struct RestoreCheckpointTool {
    base_dir: PathBuf,
}

impl RestoreCheckpointTool {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for RestoreCheckpointTool {
    fn name(&self) -> &str {
        "restore_checkpoint"
    }

    fn description(&self) -> &str {
        "Restore the workspace working tree to a checkpoint created with \
         `create_checkpoint`. Mode \"files\" (default) makes the workspace match \
         the snapshot exactly, including removing files added after it. This \
         OVERWRITES workspace files but never touches the user's real git history."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["checkpoint_id"],
            "properties": {
                "checkpoint_id": { "type": "string" },
                "mode": { "type": "string", "enum": ["files"], "default": "files" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let checkpoint_id = required_string(self.name(), &input, "checkpoint_id")?;
        let mode = RestoreMode::parse(optional_string(&input, "mode"))?;
        let store = store_for(&self.base_dir, workspace);
        let outcome = store.restore(checkpoint_id, mode)?;
        let mut value = json!({
            "checkpoint_id": outcome.id,
            "restored": true,
        });
        if let Some(count) = outcome.files_restored {
            value["files_restored"] = json!(count);
        }
        Ok(ToolResult::json(value))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    /// A fresh shadow base + a workspace tempdir with one file. Returns both
    /// tempdirs (kept alive) plus the store and a [`Workspace`] over the tree.
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Workspace) {
        let base = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        fs::write(work.path().join("file.txt"), "original\n").unwrap();
        let workspace = Workspace::new(work.path()).unwrap();
        (base, work, workspace)
    }

    fn store(base: &tempfile::TempDir, workspace: &Workspace) -> CheckpointStore {
        CheckpointStore::new(base.path(), workspace.root())
    }

    #[test]
    fn snapshot_then_modify_then_restore_brings_content_back() {
        let (base, work, ws) = fixture();
        let store = store(&base, &ws);

        let cp = store.snapshot(Some("before edit")).unwrap();
        fs::write(work.path().join("file.txt"), "CHANGED\n").unwrap();

        let outcome = store.restore(&cp.id, RestoreMode::Files).unwrap();
        let restored = fs::read_to_string(work.path().join("file.txt")).unwrap();
        assert_eq!(restored, "original\n");
        assert_eq!(outcome.files_restored, Some(1));
    }

    #[test]
    fn snapshot_captures_untracked_files_and_restore_removes_later_additions() {
        let (base, work, ws) = fixture();
        let store = store(&base, &ws);

        // `extra.txt` is untracked relative to any real repo, yet the snapshot
        // must capture it (shadow `add -A`).
        fs::write(work.path().join("extra.txt"), "hello\n").unwrap();
        let cp = store.snapshot(None).unwrap();

        // Add a NEW file after the snapshot; restore must delete it.
        fs::write(work.path().join("new.txt"), "added later\n").unwrap();
        store.restore(&cp.id, RestoreMode::Files).unwrap();

        assert!(work.path().join("extra.txt").exists(), "untracked captured");
        assert_eq!(
            fs::read_to_string(work.path().join("extra.txt")).unwrap(),
            "hello\n"
        );
        assert!(
            !work.path().join("new.txt").exists(),
            "files added after the snapshot are removed on restore"
        );
    }

    #[test]
    fn list_returns_snapshots_newest_first() {
        let (base, work, ws) = fixture();
        let store = store(&base, &ws);

        let first = store.snapshot(Some("one")).unwrap();
        fs::write(work.path().join("file.txt"), "v2\n").unwrap();
        let second = store.snapshot(Some("two")).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        // Newest first.
        assert_eq!(list[0].id, second.id);
        assert_eq!(list[0].label.as_deref(), Some("two"));
        assert_eq!(list[1].id, first.id);
        assert_eq!(list[1].label.as_deref(), Some("one"));
    }

    #[test]
    fn list_on_empty_store_is_empty_not_error() {
        let (base, _work, ws) = fixture();
        let store = store(&base, &ws);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn restore_of_unknown_id_errors_cleanly() {
        let (base, _work, ws) = fixture();
        let store = store(&base, &ws);
        store.snapshot(None).unwrap();

        let err = store
            .restore(
                "0000000000000000000000000000000000000000",
                RestoreMode::Files,
            )
            .unwrap_err();
        match err {
            HarnessError::ToolFailed { message, .. } => {
                assert!(message.contains("unknown checkpoint"), "got: {message}");
            }
            other => panic!("expected ToolFailed, got {other:?}"),
        }
    }

    /// The shadow store must never create a `.git` inside the workspace.
    #[test]
    fn shadow_ops_create_no_dot_git_in_workspace() {
        let (base, work, ws) = fixture();
        let store = store(&base, &ws);
        store.snapshot(None).unwrap();
        fs::write(work.path().join("file.txt"), "x\n").unwrap();
        let cp = store.list().unwrap();
        store.restore(&cp[0].id, RestoreMode::Files).unwrap();

        assert!(
            !work.path().join(".git").exists(),
            "shadow ops must not create a .git in the workspace"
        );
        assert!(
            store.shadow_git_dir().exists(),
            "shadow git-dir lives outside"
        );
    }

    /// Isolation: a workspace that is itself a real git repo must be unaffected
    /// by checkpoint create/restore — its HEAD, log, and status are unchanged,
    /// and only the working tree is rolled back.
    #[test]
    fn real_repo_history_is_untouched_by_checkpoints() {
        let base = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let root = work.path();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {:?}", out);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "real@example.com"]);
        run(&["config", "user.name", "Real User"]);
        fs::write(root.join("file.txt"), "v1\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "real commit"]);

        let head_before = run(&["rev-parse", "HEAD"]);
        let log_before = run(&["log", "--format=%H %s"]);
        let status_before = run(&["status", "--porcelain=v1"]);

        let ws = Workspace::new(root).unwrap();
        let store = CheckpointStore::new(base.path(), ws.root());

        // Create a checkpoint, change the working tree, then restore.
        let cp = store.snapshot(Some("ckpt")).unwrap();
        fs::write(root.join("file.txt"), "v2-changed\n").unwrap();
        store.restore(&cp.id, RestoreMode::Files).unwrap();

        // Working tree was rolled back…
        assert_eq!(
            fs::read_to_string(root.join("file.txt")).unwrap(),
            "v1\n",
            "working tree restored to checkpoint"
        );

        // …but the real repo's history/index/status is byte-for-byte unchanged.
        assert_eq!(run(&["rev-parse", "HEAD"]), head_before, "HEAD unchanged");
        assert_eq!(run(&["log", "--format=%H %s"]), log_before, "log unchanged");
        assert_eq!(
            run(&["status", "--porcelain=v1"]),
            status_before,
            "status unchanged (clean)"
        );
        // The shadow repo excludes `.git/`, so it never captured the real repo's
        // internals; the real `.git` is right where it was.
        assert!(root.join(".git").is_dir(), "real .git intact");
    }

    /// SAFETY-CRITICAL regression: if the user commits to the real repo *after* a
    /// checkpoint and then restores it, the new commit must survive — restoring a
    /// checkpoint rolls back the working tree, never the real git history. This
    /// fails if the shadow repo captures `.git/` (then `reset --hard`/`clean`
    /// would revert/delete real objects).
    #[test]
    fn real_repo_commit_after_checkpoint_survives_restore() {
        let base = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let root = work.path();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {:?}", out);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "real@example.com"]);
        run(&["config", "user.name", "Real User"]);
        fs::write(root.join("file.txt"), "v1\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "commit A"]);

        let store = CheckpointStore::new(base.path(), Workspace::new(root).unwrap().root());
        let cp = store.snapshot(Some("before B")).unwrap();

        // The user commits B to the REAL repo after the checkpoint.
        fs::write(root.join("file.txt"), "v2\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "commit B"]);
        let commit_b = run(&["rev-parse", "HEAD"]);

        // Restore the older checkpoint.
        store.restore(&cp.id, RestoreMode::Files).unwrap();

        // Commit B must still exist and still be HEAD — the real history is intact.
        assert_eq!(
            run(&["rev-parse", "HEAD"]),
            commit_b,
            "commit B is still HEAD"
        );
        assert!(
            run(&["log", "--format=%s"]).contains("commit B"),
            "commit B survives in the real repo log"
        );
        // Its object is reachable (would be gone if the shadow had cleaned .git).
        Command::new("git")
            .args(["cat-file", "-e", &commit_b])
            .current_dir(root)
            .status()
            .map(|s| assert!(s.success(), "commit B object must still exist"))
            .unwrap();
        assert!(root.join(".git").is_dir(), "real .git intact");
    }

    #[tokio::test]
    async fn tools_are_callable_end_to_end() {
        let base = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        fs::write(work.path().join("file.txt"), "original\n").unwrap();
        let ws = Workspace::new(work.path()).unwrap();

        let create = CreateCheckpointTool::new(base.path());
        let result = create.execute(&ws, json!({ "label": "v1" })).await.unwrap();
        let id = result.content()["id"].as_str().unwrap().to_string();
        assert_eq!(result.content()["label"], "v1");
        assert_eq!(
            create.permission_scope(&json!({})),
            PermissionScope::ReadOnly
        );

        // Modify, then list shows the snapshot.
        fs::write(work.path().join("file.txt"), "CHANGED\n").unwrap();
        let list = ListCheckpointsTool::new(base.path());
        let listed = list.execute(&ws, json!({})).await.unwrap();
        let arr = listed.content()["checkpoints"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], id);

        // Restore brings the file back.
        let restore = RestoreCheckpointTool::new(base.path());
        assert_eq!(
            restore.permission_scope(&json!({})),
            PermissionScope::WorkspaceWrite
        );
        let restored = restore
            .execute(&ws, json!({ "checkpoint_id": id }))
            .await
            .unwrap();
        assert_eq!(restored.content()["restored"], true);
        assert_eq!(
            fs::read_to_string(work.path().join("file.txt")).unwrap(),
            "original\n"
        );

        // Bad mode is a clean input error.
        let err = restore
            .execute(&ws, json!({ "checkpoint_id": id, "mode": "nope" }))
            .await
            .unwrap_err();
        assert!(matches!(err, HarnessError::InvalidToolInput { .. }));
    }
}

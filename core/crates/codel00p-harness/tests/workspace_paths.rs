use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use codel00p_harness::{
    ChildHandle, CommandOutcome, CommandSpec, DirEntry, FileKind, HarnessError, LocalBackend,
    OutputLimits, TerminalBackend, Workspace,
};
use tempfile::tempdir;

#[test]
fn resolves_relative_paths_inside_workspace_root() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("create src");
    fs::write(dir.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("write file");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let resolved = workspace.resolve("src/lib.rs").expect("resolve path");

    assert_eq!(
        resolved,
        dir.path().join("src/lib.rs").canonicalize().unwrap()
    );
}

#[test]
fn rejects_parent_directory_traversal() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let error = workspace
        .resolve("../outside.txt")
        .expect_err("escape rejected");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[test]
fn rejects_absolute_paths_outside_workspace() {
    let workspace_dir = tempdir().expect("workspace tempdir");
    let outside_dir = tempdir().expect("outside tempdir");
    let outside_file = outside_dir.path().join("secret.txt");
    fs::write(&outside_file, "secret").expect("write outside file");

    let workspace = Workspace::new(workspace_dir.path()).expect("workspace");
    let error = workspace
        .resolve(&outside_file)
        .expect_err("outside path rejected");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[test]
fn reads_utf8_files_inside_workspace() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let content = workspace.read_utf8("README.md").expect("read file");

    assert_eq!(content, "Agent Harness\n");
}

/// A backend that counts fs calls and otherwise delegates to a rooted
/// [`LocalBackend`], used to prove `Workspace::with_backend` routes I/O through
/// the supplied backend rather than touching `std::fs` directly.
struct CountingBackend {
    inner: LocalBackend,
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl CountingBackend {
    fn new(root: &Path) -> Self {
        Self {
            inner: LocalBackend::rooted(root.canonicalize().unwrap()),
            reads: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
        }
    }
}

impl TerminalBackend for CountingBackend {
    fn run_foreground(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError> {
        self.inner.run_foreground(spec, limits)
    }
    fn spawn_background(&self, spec: &CommandSpec) -> Result<Box<dyn ChildHandle>, HarnessError> {
        self.inner.spawn_background(spec)
    }
    fn read_file(&self, rel: &Path) -> Result<Vec<u8>, HarnessError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read_file(rel)
    }
    fn write_file(&self, rel: &Path, contents: &[u8]) -> Result<(), HarnessError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.write_file(rel, contents)
    }
    fn delete_file(&self, rel: &Path) -> Result<(), HarnessError> {
        self.inner.delete_file(rel)
    }
    fn create_dir_all(&self, rel: &Path) -> Result<(), HarnessError> {
        self.inner.create_dir_all(rel)
    }
    fn metadata(&self, rel: &Path) -> Result<FileKind, HarnessError> {
        self.inner.metadata(rel)
    }
    fn read_dir(&self, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
        self.inner.read_dir(rel)
    }
}

#[test]
fn with_backend_routes_io_through_the_supplied_backend() {
    let dir = tempdir().expect("tempdir");
    let backend = Arc::new(CountingBackend::new(dir.path()));
    let workspace =
        Workspace::with_backend(dir.path(), backend.clone()).expect("workspace with backend");

    workspace
        .write("note.txt", "hi")
        .expect("write via backend");
    let content = workspace.read_utf8("note.txt").expect("read via backend");

    assert_eq!(content, "hi");
    assert_eq!(backend.writes.load(Ordering::SeqCst), 1);
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    // And it landed on disk at the real workspace path.
    assert_eq!(
        fs::read_to_string(dir.path().join("note.txt")).unwrap(),
        "hi"
    );
}

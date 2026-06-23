//! Workspace mutation tools.
//!
//! Layered so each concern reads on its own:
//!
//! * [`patch`] — the pure find/replace engine (no I/O), exhaustively unit-tested;
//! * [`apply_patch`] — the `apply_patch` tool: schema, tolerant normalization, and
//!   atomic batch application over the engine;
//! * [`file_ops`] — the whole-file tools (create/update/delete/move/copy);
//! * [`support`] — the file-existence and destination guards they share.
//!
//! The tool structs are re-exported here so the rest of the crate keeps importing
//! them from `crate::editing`.

mod apply_patch;
mod file_ops;
mod patch;
mod support;

pub use apply_patch::ApplyPatchTool;
pub use file_ops::{CopyFileTool, CreateFileTool, DeleteFileTool, MoveFileTool, UpdateFileTool};

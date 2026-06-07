use std::io;

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("invalid harness configuration: {message}")]
    Configuration { message: String },

    #[error("workspace path escapes the configured root: {path}")]
    WorkspaceEscape { path: String },

    #[error("workspace path is invalid: {path}")]
    InvalidWorkspacePath { path: String },

    #[error("tool not found: {name}")]
    ToolNotFound { name: String },

    #[error("invalid input for tool {name}: {message}")]
    InvalidToolInput { name: String, message: String },

    #[error("tool failed: {name}: {message}")]
    ToolFailed { name: String, message: String },

    #[error("inference failed: {message}")]
    InferenceFailed { message: String },

    #[error("turn reached iteration limit: {limit}")]
    IterationLimit { limit: u32 },

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

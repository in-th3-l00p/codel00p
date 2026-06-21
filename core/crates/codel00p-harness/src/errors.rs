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

    #[error(
        "reached the {limit}-step iteration limit before finishing — the task needed more steps than allowed. Raise it with `--max-iterations`, `agent.max_iterations`, or Settings → Advanced, or break the task into smaller steps."
    )]
    IterationLimit { limit: u32 },

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::HarnessError;

    #[test]
    fn iteration_limit_display_is_actionable() {
        let message = HarnessError::IterationLimit { limit: 25 }.to_string();
        // The terse limit value is still present.
        assert!(message.contains("25"));
        // ...but now framed actionably with concrete next steps.
        assert!(message.contains("iteration limit"));
        assert!(message.contains("--max-iterations"));
        assert!(message.contains("agent.max_iterations"));
        assert!(message.contains("Settings → Advanced"));
        assert!(message.contains("smaller steps"));
    }
}

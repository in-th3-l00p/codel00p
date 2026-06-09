use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    content: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    progress: Vec<ToolResultProgress>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultProgress {
    phase: String,
    message: Option<String>,
}

impl ToolResult {
    pub fn json(content: Value) -> Self {
        Self {
            content,
            progress: Vec::new(),
        }
    }

    pub fn with_progress(
        mut self,
        phase: impl Into<String>,
        message: Option<impl Into<String>>,
    ) -> Self {
        self.progress.push(ToolResultProgress {
            phase: phase.into(),
            message: message.map(Into::into),
        });
        self
    }

    pub fn content(&self) -> &Value {
        &self.content
    }

    pub fn progress(&self) -> &[ToolResultProgress] {
        &self.progress
    }
}

impl ToolResultProgress {
    pub fn phase(&self) -> &str {
        &self.phase
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

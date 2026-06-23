//! The tool contract and the helpers every tool shares.
//!
//! [`Tool`] is the trait each workspace capability implements; [`ToolSpec`] is
//! the model-facing definition derived from it. The rest of this module is the
//! small shared vocabulary tools reuse: argument readers (`required_string`,
//! `optional_string`) and the `verbosity` token-efficiency control. Concrete
//! tools live in their own modules (e.g. [`read`] here, `crate::editing`, …); the
//! read-only inspection tools are re-exported below so callers keep importing
//! them from `crate::tools`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, workspace::Workspace};
use codel00p_protocol::PermissionScope;

mod read;
pub use read::{ListFilesTool, ReadFileTool, SearchTextTool};

/// A tool's model-facing definition: the name, description, and JSON Schema the
/// model needs to call it. Produced from a [`Tool`] and sent to the provider, so
/// the model sees real parameters instead of a stub.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Builds a spec from a tool's trait methods.
    pub fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn input_schema(&self) -> Value;

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        false
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError>;
}

/// Token-efficiency control shared by the heavy read/output tools.
///
/// `Detailed` is the default everywhere and is byte-identical to each tool's
/// historical output. `Concise` trims a tool's result to a cheaper shape (the
/// exact trimming is tool-specific and documented at each call site) so the
/// model can ask for a summary instead of always paying for full output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verbosity {
    Concise,
    Detailed,
}

impl Verbosity {
    pub(crate) fn is_concise(self) -> bool {
        matches!(self, Verbosity::Concise)
    }
}

/// Parse the shared `verbosity` field. Accepts `"concise"` or `"detailed"`;
/// an omitted field defaults to `Detailed` (no behavior change). Any other
/// value is rejected so typos surface instead of silently falling back.
pub(crate) fn parse_verbosity(tool: &str, input: &Value) -> Result<Verbosity, HarnessError> {
    match optional_string(input, "verbosity") {
        None => Ok(Verbosity::Detailed),
        Some("detailed") => Ok(Verbosity::Detailed),
        Some("concise") => Ok(Verbosity::Concise),
        Some(other) => Err(HarnessError::InvalidToolInput {
            name: tool.to_string(),
            message: format!("`verbosity` must be \"concise\" or \"detailed\", got `{other}`"),
        }),
    }
}

/// The JSON Schema fragment for the shared `verbosity` field, so every tool that
/// supports it advertises the same enum and default.
pub(crate) fn verbosity_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["concise", "detailed"],
        "default": "detailed"
    })
}

pub(crate) fn required_string<'a>(
    tool: &str,
    input: &'a Value,
    key: &str,
) -> Result<&'a str, HarnessError> {
    optional_string(input, key).ok_or_else(|| HarnessError::InvalidToolInput {
        name: tool.to_string(),
        message: format!("missing string field `{key}`"),
    })
}

pub(crate) fn optional_string<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_verbosity_defaults_and_rejects_garbage() {
        assert_eq!(
            parse_verbosity("t", &json!({})).unwrap(),
            Verbosity::Detailed
        );
        assert_eq!(
            parse_verbosity("t", &json!({ "verbosity": "concise" })).unwrap(),
            Verbosity::Concise
        );
        assert_eq!(
            parse_verbosity("t", &json!({ "verbosity": "detailed" })).unwrap(),
            Verbosity::Detailed
        );
        let err = parse_verbosity("t", &json!({ "verbosity": "loud" })).unwrap_err();
        assert!(matches!(err, HarnessError::InvalidToolInput { .. }));
    }
}

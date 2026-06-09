use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 16_384;
const MAX_OUTPUT_BYTES: usize = 131_072;
const POLL_INTERVAL_MS: u64 = 10;

pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Run a command inside the workspace with timeout and output limits."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["program"],
            "properties": {
                "program": { "type": "string" },
                "args": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "cwd": { "type": "string" },
                "timeout_ms": { "type": "integer" },
                "max_output_bytes": { "type": "integer" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::Shell
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let program = required_string(self.name(), &input, "program")?;
        if program.trim().is_empty() {
            return Err(HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "`program` must not be empty".to_string(),
            });
        }

        let args = optional_string_array(self.name(), &input, "args")?;
        let cwd = optional_string(&input, "cwd").unwrap_or(".");
        let working_dir = workspace.resolve_directory(cwd)?;
        let timeout = Duration::from_millis(
            optional_u64(&input, "timeout_ms", DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS),
        );
        let max_output_bytes = optional_usize(&input, "max_output_bytes", DEFAULT_MAX_OUTPUT_BYTES)
            .min(MAX_OUTPUT_BYTES);

        let mut child = Command::new(program)
            .args(&args)
            .current_dir(&working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| HarnessError::ToolFailed {
                name: self.name().to_string(),
                message: error.to_string(),
            })?;

        let started = Instant::now();
        let mut timed_out = false;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if started.elapsed() >= timeout {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        let output = child.wait_with_output()?;
        let stdout = cap_output(&output.stdout, max_output_bytes);
        let stderr = cap_output(&output.stderr, max_output_bytes);
        let exit_code = if timed_out {
            None
        } else {
            output.status.code()
        };
        let success = !timed_out && output.status.success();

        Ok(ToolResult::json(json!({
            "program": program,
            "args": args,
            "cwd": cwd,
            "exit_code": exit_code,
            "success": success,
            "timed_out": timed_out,
            "stdout": stdout.content,
            "stderr": stderr.content,
            "stdout_truncated": stdout.truncated,
            "stderr_truncated": stderr.truncated,
        })))
    }
}

struct CappedOutput {
    content: String,
    truncated: bool,
}

fn cap_output(bytes: &[u8], max_bytes: usize) -> CappedOutput {
    let truncated = bytes.len() > max_bytes;
    let end = bytes.len().min(max_bytes);
    CappedOutput {
        content: String::from_utf8_lossy(&bytes[..end]).to_string(),
        truncated,
    }
}

fn optional_string_array(
    tool_name: &str,
    input: &Value,
    key: &str,
) -> Result<Vec<String>, HarnessError> {
    let Some(value) = input.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| HarnessError::InvalidToolInput {
            name: tool_name.to_string(),
            message: format!("`{key}` must be an array of strings"),
        })?;

    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| HarnessError::InvalidToolInput {
                    name: tool_name.to_string(),
                    message: format!("`{key}` must be an array of strings"),
                })
        })
        .collect()
}

fn optional_u64(input: &Value, key: &str, default: u64) -> u64 {
    input.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn optional_usize(input: &Value, key: &str, default: usize) -> usize {
    input
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

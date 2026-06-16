use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    background::{BackgroundProcesses, ProcessStatus},
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

/// Runs commands inside the workspace. Foreground calls block until the command
/// exits (or times out); `background: true` spawns the command, returns a
/// `process_id` immediately, and registers it with the shared
/// [`BackgroundProcesses`] store the `process_*` tools read from.
pub struct RunCommandTool {
    processes: BackgroundProcesses,
}

impl RunCommandTool {
    pub fn new(processes: BackgroundProcesses) -> Self {
        Self { processes }
    }
}

impl Default for RunCommandTool {
    fn default() -> Self {
        Self::new(BackgroundProcesses::new())
    }
}

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Run a command inside the workspace. Blocks until the command exits or \
         `timeout_ms` elapses, returning its exit code and (capped) output. For a \
         long-running process that does not exit on its own (a dev server, a \
         watcher), set `background: true`: the command is spawned and a \
         `process_id` is returned immediately — poll it with `process_output`, see \
         running processes with `process_list`, and stop it with `process_kill`."
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
                "max_output_bytes": { "type": "integer" },
                "background": { "type": "boolean" }
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

        // Background path: spawn, register, and return a handle without waiting.
        if input
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let label = command_label(program, &args);
            let process_id = self
                .processes
                .spawn(program, &args, &working_dir, label.clone())?;
            return Ok(ToolResult::json(json!({
                "program": program,
                "args": args,
                "cwd": cwd,
                "background": true,
                "process_id": process_id,
                "status": "running",
                "hint": "poll `process_output` with this process_id; stop with `process_kill`",
            })));
        }

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

/// A short human label for a spawned command, used in process listings.
fn command_label(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

/// Render a [`ProcessStatus`] to the JSON the process tools return.
fn status_json(status: ProcessStatus) -> Value {
    match status {
        ProcessStatus::Running => json!({ "state": "running" }),
        ProcessStatus::Exited { code, killed } => json!({
            "state": if killed { "killed" } else { "exited" },
            "exit_code": code,
            "killed": killed,
        }),
    }
}

/// Read incremental output from a background process started by `run_command`.
pub struct ProcessOutputTool {
    processes: BackgroundProcesses,
}

impl ProcessOutputTool {
    pub fn new(processes: BackgroundProcesses) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl Tool for ProcessOutputTool {
    fn name(&self) -> &str {
        "process_output"
    }

    fn description(&self) -> &str {
        "Read output from a background process started by `run_command` with \
         `background: true`. Returns only the stdout/stderr produced since the \
         previous read for this `process_id`, plus whether the process is still \
         running or has exited (with its exit code)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["process_id"],
            "properties": {
                "process_id": { "type": "string" },
                "max_output_bytes": { "type": "integer" }
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let process_id = required_string(self.name(), &input, "process_id")?;
        let max_output_bytes = optional_usize(&input, "max_output_bytes", DEFAULT_MAX_OUTPUT_BYTES)
            .min(MAX_OUTPUT_BYTES);

        let snapshot = self
            .processes
            .output(process_id, max_output_bytes)
            .ok_or_else(|| HarnessError::ToolFailed {
                name: self.name().to_string(),
                message: format!("no background process with id `{process_id}`"),
            })?;

        Ok(ToolResult::json(json!({
            "process_id": process_id,
            "label": snapshot.label,
            "status": status_json(snapshot.status),
            "stdout": snapshot.stdout,
            "stderr": snapshot.stderr,
            "stdout_truncated": snapshot.stdout_truncated,
            "stderr_truncated": snapshot.stderr_truncated,
        })))
    }
}

/// List the background processes started this session and their status.
pub struct ProcessListTool {
    processes: BackgroundProcesses,
}

impl ProcessListTool {
    pub fn new(processes: BackgroundProcesses) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl Tool for ProcessListTool {
    fn name(&self) -> &str {
        "process_list"
    }

    fn description(&self) -> &str {
        "List the background processes started by `run_command` this session, with \
         each one's id, command label, and current status (running or exited)."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let processes: Vec<Value> = self
            .processes
            .list()
            .into_iter()
            .map(|info| {
                json!({
                    "process_id": info.id,
                    "label": info.label,
                    "status": status_json(info.status),
                })
            })
            .collect();
        Ok(ToolResult::json(json!({ "processes": processes })))
    }
}

/// Terminate a background process started by `run_command`.
pub struct ProcessKillTool {
    processes: BackgroundProcesses,
}

impl ProcessKillTool {
    pub fn new(processes: BackgroundProcesses) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl Tool for ProcessKillTool {
    fn name(&self) -> &str {
        "process_kill"
    }

    fn description(&self) -> &str {
        "Stop a background process started by `run_command`, identified by its \
         `process_id`. Returns the process's final status."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["process_id"],
            "properties": {
                "process_id": { "type": "string" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::Shell
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let process_id = required_string(self.name(), &input, "process_id")?;
        let status = self
            .processes
            .kill(process_id)
            .ok_or_else(|| HarnessError::ToolFailed {
                name: self.name().to_string(),
                message: format!("no background process with id `{process_id}`"),
            })?;
        Ok(ToolResult::json(json!({
            "process_id": process_id,
            "status": status_json(status),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        (dir, ws)
    }

    /// Poll `process_output` until the process exits or the budget runs out.
    async fn wait_for_exit(
        tool: &ProcessOutputTool,
        ws: &Workspace,
        id: &str,
    ) -> serde_json::Value {
        let mut stdout = String::new();
        for _ in 0..200 {
            let result = tool.execute(ws, json!({ "process_id": id })).await.unwrap();
            let content = result.content().clone();
            stdout.push_str(content["stdout"].as_str().unwrap_or(""));
            if content["status"]["state"] != "running" {
                // Drain any final buffered output once more.
                let tail = tool.execute(ws, json!({ "process_id": id })).await.unwrap();
                stdout.push_str(tail.content()["stdout"].as_str().unwrap_or(""));
                return json!({ "status": content["status"], "stdout": stdout });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("background process did not exit in time");
    }

    #[tokio::test]
    async fn background_command_runs_and_reports_output() {
        let (_dir, ws) = workspace();
        let processes = BackgroundProcesses::new();
        let run = RunCommandTool::new(processes.clone());
        let output = ProcessOutputTool::new(processes.clone());

        let spawned = run
            .execute(
                &ws,
                json!({
                    "program": "sh",
                    "args": ["-c", "printf hello; printf err 1>&2"],
                    "background": true
                }),
            )
            .await
            .unwrap();
        let id = spawned.content()["process_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(spawned.content()["background"], true);

        let final_state = wait_for_exit(&output, &ws, &id).await;
        assert_eq!(final_state["status"]["state"], "exited");
        assert_eq!(final_state["status"]["exit_code"], 0);
        assert!(final_state["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn process_list_and_kill_stop_a_running_process() {
        let (_dir, ws) = workspace();
        let processes = BackgroundProcesses::new();
        let run = RunCommandTool::new(processes.clone());
        let list = ProcessListTool::new(processes.clone());
        let kill = ProcessKillTool::new(processes.clone());

        let spawned = run
            .execute(
                &ws,
                json!({ "program": "sh", "args": ["-c", "sleep 30"], "background": true }),
            )
            .await
            .unwrap();
        let id = spawned.content()["process_id"]
            .as_str()
            .unwrap()
            .to_string();

        let listed = list.execute(&ws, json!({})).await.unwrap();
        let processes_json = listed.content()["processes"].as_array().unwrap();
        assert_eq!(processes_json.len(), 1);
        assert_eq!(processes_json[0]["status"]["state"], "running");

        let killed = kill
            .execute(&ws, json!({ "process_id": id }))
            .await
            .unwrap();
        assert_eq!(killed.content()["status"]["state"], "killed");
    }

    #[tokio::test]
    async fn process_output_unknown_id_errors() {
        let (_dir, ws) = workspace();
        let output = ProcessOutputTool::new(BackgroundProcesses::new());
        let error = output
            .execute(&ws, json!({ "process_id": "proc-999" }))
            .await
            .unwrap_err();
        assert!(matches!(error, HarnessError::ToolFailed { .. }));
    }

    #[tokio::test]
    async fn foreground_command_still_works() {
        let (_dir, ws) = workspace();
        let run = RunCommandTool::default();
        let result = run
            .execute(&ws, json!({ "program": "sh", "args": ["-c", "echo hi"] }))
            .await
            .unwrap();
        assert_eq!(result.content()["success"], true);
        assert!(result.content()["stdout"].as_str().unwrap().contains("hi"));
    }
}

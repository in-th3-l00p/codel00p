//! Local and gateway permission policies for agent tool execution.

use super::*;

pub(super) struct GatewayApprovalPolicy {
    store: ApprovalStore,
    conversation: String,
    /// `Some(tool)` until the matching tool runs once, then taken.
    granted_tool: Mutex<Option<String>>,
}

impl GatewayApprovalPolicy {
    pub(super) fn new(
        store: ApprovalStore,
        conversation: String,
        granted_tool: Option<String>,
    ) -> Self {
        Self {
            store,
            conversation,
            granted_tool: Mutex::new(granted_tool),
        }
    }
}

/// A short, single-line description of what a tool wants to do, shown to the
/// remote user in the approval prompt.
fn describe_permission_request(request: &PermissionRequest) -> String {
    let input = request.input();
    // Prefer a `command` field (shell) for a crisp summary; otherwise show the
    // compact tool input, truncated so a chat prompt stays readable.
    if let Some(command) = input.get("command").and_then(|value| value.as_str()) {
        return command.trim().to_string();
    }
    let mut rendered = match input {
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    const MAX: usize = 200;
    if rendered.chars().count() > MAX {
        rendered = rendered.chars().take(MAX).collect::<String>() + "…";
    }
    rendered
}

#[async_trait]
impl PermissionPolicy for GatewayApprovalPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        // Reading never needs a remote user's blessing.
        if request.scope() == PermissionScope::ReadOnly {
            return Ok(PermissionDecision::allow(
                request.id(),
                PermissionMode::Allow,
            ));
        }
        // Consume a one-shot grant for exactly the tool the user just approved.
        {
            let mut granted = self
                .granted_tool
                .lock()
                .map_err(|_| HarnessError::ToolFailed {
                    name: request.tool_name().to_string(),
                    message: "gateway approval lock was poisoned".to_string(),
                })?;
            if granted.as_deref() == Some(request.tool_name()) {
                *granted = None;
                return Ok(PermissionDecision::allow(request.id(), PermissionMode::Ask));
            }
        }
        // Otherwise park the turn: record what is wanted and deny for now.
        self.store
            .record(
                &self.conversation,
                request.tool_name(),
                &describe_permission_request(&request),
            )
            .map_err(|error| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: format!("failed to record approval request: {error}"),
            })?;
        Ok(PermissionDecision::deny(
            request.id(),
            PermissionMode::Ask,
            format!("awaiting remote /approve for {}", request.tool_name()),
        ))
    }
}

pub(crate) struct CliPermissionPolicy {
    config: CliConfig,
    mode: CliPermissionMode,
    remember_permissions: bool,
    prompt_lock: Arc<Mutex<()>>,
}

impl CliPermissionPolicy {
    pub(crate) fn new(
        config: CliConfig,
        mode: CliPermissionMode,
        remember_permissions: bool,
    ) -> Self {
        Self {
            config,
            mode,
            remember_permissions,
            prompt_lock: Arc::new(Mutex::new(())),
        }
    }
}

#[async_trait]
impl PermissionPolicy for CliPermissionPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        if let Some(decision) = self.fast_path(&request)? {
            return Ok(decision);
        }
        // Only Ask mode without a remembered decision reaches the stdin prompt.
        let decision = self.decide_with_prompt(&request)?;
        self.persist_decision_if_needed(&request, &decision)?;
        Ok(decision)
    }
}

impl CliPermissionPolicy {
    /// Resolve without any interactive prompt when possible. Returns `None` only
    /// when Ask mode needs a fresh approval, which either stdin or the TUI handles.
    pub(crate) fn fast_path(
        &self,
        request: &PermissionRequest,
    ) -> Result<Option<PermissionDecision>, HarnessError> {
        match self.mode {
            CliPermissionMode::Allow => Ok(Some(PermissionDecision::allow(
                request.id(),
                PermissionMode::Allow,
            ))),
            CliPermissionMode::Ask => self.remembered_decision(request),
            CliPermissionMode::Deny => Ok(Some(PermissionDecision::deny(
                request.id(),
                PermissionMode::Deny,
                format!("{} denied by CLI permission mode", request.tool_name()),
            ))),
        }
    }

    fn remembered_decision(
        &self,
        request: &PermissionRequest,
    ) -> Result<Option<PermissionDecision>, HarnessError> {
        if !self.should_remember(request) {
            return Ok(None);
        }
        let decision =
            load_decision(&self.config, request.tool_name(), request.scope()).map_err(|error| {
                HarnessError::ToolFailed {
                    name: request.tool_name().to_string(),
                    message: format!("failed to read remembered permission: {error}"),
                }
            })?;
        Ok(match decision.map(|decision| decision.status) {
            Some(ConnectorPermissionStatus::Allow) => {
                Some(PermissionDecision::allow(request.id(), PermissionMode::Ask))
            }
            Some(ConnectorPermissionStatus::Deny) => Some(PermissionDecision::deny(
                request.id(),
                PermissionMode::Ask,
                format!(
                    "{} denied by remembered connector policy",
                    request.tool_name()
                ),
            )),
            None => None,
        })
    }

    pub(crate) fn persist_decision_if_needed(
        &self,
        request: &PermissionRequest,
        decision: &PermissionDecision,
    ) -> Result<(), HarnessError> {
        if !self.should_remember(request) {
            return Ok(());
        }
        let status = if decision.allows_execution() {
            ConnectorPermissionStatus::Allow
        } else {
            ConnectorPermissionStatus::Deny
        };
        remember_decision(
            &self.config,
            ConnectorPermissionDecision {
                tool_name: request.tool_name().to_string(),
                scope: request.scope(),
                status,
            },
        )
        .map_err(|error| HarnessError::ToolFailed {
            name: request.tool_name().to_string(),
            message: format!("failed to remember permission: {error}"),
        })?;
        Ok(())
    }

    fn should_remember(&self, request: &PermissionRequest) -> bool {
        self.remember_permissions
            && is_rememberable_permission(request.tool_name(), request.scope())
    }

    fn decide_with_prompt(
        &self,
        request: &PermissionRequest,
    ) -> Result<PermissionDecision, HarnessError> {
        let _prompt = self
            .prompt_lock
            .lock()
            .map_err(|_| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: "permission prompt lock was poisoned".to_string(),
            })?;

        let approved =
            prompt_for_permission(request).map_err(|error| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: format!("failed to read permission approval: {error}"),
            })?;

        if approved {
            Ok(PermissionDecision::allow(request.id(), PermissionMode::Ask))
        } else {
            Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Ask,
                format!("{} rejected by CLI approval prompt", request.tool_name()),
            ))
        }
    }
}

fn prompt_for_permission(request: &PermissionRequest) -> io::Result<bool> {
    let mut stderr = io::stderr();
    write!(
        stderr,
        "Allow tool `{}` for {:?}? [y/N] ",
        request.tool_name(),
        request.scope()
    )?;
    stderr.flush()?;

    let mut answer = String::new();
    let bytes = io::stdin().read_line(&mut answer)?;
    if bytes == 0 {
        return Ok(false);
    }

    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

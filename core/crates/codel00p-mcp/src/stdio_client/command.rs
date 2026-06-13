use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpClientRoot {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl McpClientRoot {
    pub fn new(uri: impl Into<String>, name: Option<impl Into<String>>) -> Self {
        Self {
            uri: uri.into(),
            name: name.map(Into::into),
        }
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdioServerCommand {
    pub(super) server_id: String,
    pub(super) program: PathBuf,
    pub(super) args: Vec<OsString>,
    pub(super) env: Vec<(OsString, OsString)>,
    pub(super) roots: Vec<McpClientRoot>,
    pub(super) request_timeout: Duration,
    pub(super) shutdown_timeout: Duration,
}

impl StdioServerCommand {
    pub fn new<I, S>(server_id: impl Into<String>, program: impl Into<PathBuf>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self {
            server_id: server_id.into(),
            program: program.into(),
            args: args
                .into_iter()
                .map(|arg| arg.as_ref().to_os_string())
                .collect(),
            env: Vec::new(),
            roots: Vec::new(),
            request_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(5),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn with_env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    pub fn with_root(mut self, uri: impl Into<String>, name: Option<impl Into<String>>) -> Self {
        self.roots.push(McpClientRoot::new(uri, name));
        self
    }

    pub fn with_envs<I, K, V>(mut self, env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env.extend(
            env.into_iter()
                .map(|(key, value)| (key.as_ref().to_os_string(), value.as_ref().to_os_string())),
        );
        self
    }

    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub fn with_shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.shutdown_timeout = shutdown_timeout;
        self
    }
}

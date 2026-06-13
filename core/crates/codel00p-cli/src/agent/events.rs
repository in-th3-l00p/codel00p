//! Stdout token and event sinks used by CLI streaming flags.

use super::*;

pub(super) struct StdoutTokenSink;

impl TokenSink for StdoutTokenSink {
    fn on_token(&self, token: &str) {
        print!("{token}");
        let _ = io::stdout().flush();
    }
}

pub(super) struct StdoutJsonEventSink;

#[async_trait]
impl AgentEventSink for StdoutJsonEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        if let Ok(encoded) = serde_json::to_string(event) {
            println!("{encoded}");
        }
    }
}

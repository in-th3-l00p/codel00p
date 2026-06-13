//! Protocol version contract shared across runtime components.

use serde::{Deserialize, Serialize};

const CURRENT_PROTOCOL_VERSION: &str = "codel00p.protocol.v1";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    pub fn current() -> Self {
        Self(CURRENT_PROTOCOL_VERSION.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

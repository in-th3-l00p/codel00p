//! Runtime error categories used for recovery and reporting boundaries.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeErrorKind {
    ProviderAuth,
    ProviderRateLimit,
    ProviderBilling,
    ProviderUnavailable,
    ModelUnavailable,
    ContextOverflow,
    PayloadTooLarge,
    PermissionDenied,
    ToolExecution,
    InvalidToolInput,
    Cancelled,
    IterationLimit,
    Storage,
    Unknown,
}

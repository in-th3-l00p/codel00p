use async_trait::async_trait;
pub use codel00p_protocol::{
    PermissionDecision, PermissionMode, PermissionRequest, PermissionScope,
};

use crate::errors::HarnessError;

#[async_trait]
pub trait PermissionPolicy: Send + Sync {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError>;
}

#[derive(Clone, Default)]
pub struct AllowAllPermissionPolicy;

#[async_trait]
impl PermissionPolicy for AllowAllPermissionPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        Ok(PermissionDecision::allow(
            request.id(),
            PermissionMode::Allow,
        ))
    }
}

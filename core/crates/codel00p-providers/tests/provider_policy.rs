use codel00p_providers::{
    AuthType, ChatMessage, Credential, CredentialKind, CredentialSourceKind, InferenceClient,
    InferenceRequest, ProviderCapabilities, ProviderError, ProviderPolicy, RouteValueSource,
    default_registry,
};
use serde_json::json;

#[path = "provider_policy/core.rs"]
mod core;
#[path = "provider_policy/enterprise_credentials.rs"]
mod enterprise_credentials;
#[path = "provider_policy/enterprise_direct.rs"]
mod enterprise_direct;
#[path = "provider_policy/gateway_serialization.rs"]
mod gateway_serialization;

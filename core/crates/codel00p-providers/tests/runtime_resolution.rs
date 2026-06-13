use codel00p_providers::{
    ApiMode, AuthType, ChatMessage, Credential, CredentialKind, CredentialSourceKind,
    InferenceClient, InferenceRequest, ManagedIdentityCredentialRequest,
    ManagedIdentityCredentialResolver, ProviderCapabilities, ProviderError, ProviderPolicy,
    ProviderPolicyDecision, RouteValueSource, default_registry,
};

#[path = "runtime_resolution/credentials.rs"]
mod credentials;
#[path = "runtime_resolution/metadata.rs"]
mod metadata;
#[path = "runtime_resolution/policy_metadata.rs"]
mod policy_metadata;
#[path = "runtime_resolution/routing.rs"]
mod routing;

fn with_env_lock(test: impl FnOnce()) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let keys = [
        "CODEL00P_PROVIDER_OPENAI_API_KEY",
        "OPENAI_API_KEY",
        "CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID",
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "CODEL00P_PROVIDER_AWS_SESSION_TOKEN",
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ];
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
    test();
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

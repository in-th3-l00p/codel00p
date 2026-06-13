//! Shared imports for model catalog integration tests.

pub(crate) use codel00p_providers::{
    ApiMode, AuthType, Credential, CredentialKind, CredentialSourceKind, InferenceClient,
    ModelCatalogRequest, ModelCatalogUrlSource, OutputTokenParameter, ProviderCapabilities,
    ProviderError, ProviderPolicy, ProviderPolicyDecision, ProviderProfile, ProviderRegistry,
    default_registry,
};
pub(crate) use httpmock::Method::GET;
pub(crate) use httpmock::prelude::*;
pub(crate) use serde_json::json;

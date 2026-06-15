//! codel00p inference provider abstraction.
//!
//! The public facade is intentionally small:
//!
//! ```no_run
//! # use codel00p_providers::{ChatMessage, Credential, InferenceClient, InferenceRequest, default_registry};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = InferenceClient::builder()
//!     .registry(default_registry())
//!     .credential("openai", Credential::api_key("secret"))
//!     .build();
//!
//! let response = client
//!     .complete(
//!         InferenceRequest::builder("openai", "gpt-5")
//!             .message(ChatMessage::user("Summarize this project."))
//!             .build(),
//!     )
//!     .await?;
//! # let _ = response;
//! # Ok(())
//! # }
//! ```

mod aws_managed_identity;
mod azure_managed_identity;
mod client;
mod credentials;
mod error;
mod error_classifier;
mod gcp_managed_identity;
mod model_catalog;
mod policy;
mod pricing_catalog;
mod profile;
mod registry;
mod request;
mod response;
mod runtime;
mod stream;
mod transports;

pub use aws_managed_identity::{
    AWS_MANAGED_IDENTITY_IMDS_ENDPOINT, AwsManagedIdentityCredentialResolver,
};
pub use azure_managed_identity::{
    AZURE_MANAGED_IDENTITY_TOKEN_ENDPOINT, AzureManagedIdentityCredentialResolver,
    AzureManagedIdentitySelector,
};
pub use client::{InferenceClient, InferenceClientBuilder, RetryPolicy};
pub use credentials::{
    Credential, CredentialKind, CredentialSourceKind, ManagedIdentityCredentialRequest,
    ManagedIdentityCredentialResolver, ResolvedProviderCredential,
};
pub use error::ProviderError;
pub use error_classifier::{ClassifiedProviderError, classify_provider_error};
pub use gcp_managed_identity::{
    GCP_MANAGED_IDENTITY_METADATA_ENDPOINT, GcpManagedIdentityCredentialResolver,
};
pub use model_catalog::{
    ModelCatalogRequest, ModelCatalogRequestBuilder, ModelCatalogUrlSource, ProviderModel,
    ProviderModelAnnotations, ProviderModelCatalog, ProviderModelCatalogPolicy,
    ProviderModelLimits,
};
pub use policy::{ProviderPolicy, ProviderPolicyPreset};
pub use pricing_catalog::{ProviderModelPricing, ProviderPricingCatalog};
pub use profile::{ApiMode, AuthType, OutputTokenParameter, ProviderCapabilities, ProviderProfile};
pub use registry::{ProviderRegistry, default_registry};
pub use request::{
    ChatMessage, InferenceFallbackRoute, InferenceRequest, InferenceRequestBuilder, MessageRole,
    ToolDefinition,
};
pub use response::{InferenceResponse, ToolCall, Usage, UsageCostEstimate, UsagePricing};
pub use runtime::{
    ProviderPolicyDecision, ProviderRoutePolicy, ResolvedInferenceRoute, RouteValueSource,
};
pub use stream::TokenSink;

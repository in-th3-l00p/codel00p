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

mod client;
mod credentials;
mod error;
mod error_classifier;
mod policy;
mod profile;
mod registry;
mod request;
mod response;
mod runtime;
mod transports;

pub use client::{InferenceClient, InferenceClientBuilder};
pub use credentials::Credential;
pub use error::ProviderError;
pub use error_classifier::{ClassifiedProviderError, classify_provider_error};
pub use policy::ProviderPolicy;
pub use profile::{ApiMode, AuthType, OutputTokenParameter, ProviderCapabilities, ProviderProfile};
pub use registry::{ProviderRegistry, default_registry};
pub use request::{
    ChatMessage, InferenceFallbackRoute, InferenceRequest, InferenceRequestBuilder, MessageRole,
    ToolDefinition,
};
pub use response::{InferenceResponse, ToolCall, Usage};
pub use runtime::{ProviderPolicyDecision, ResolvedInferenceRoute, RouteValueSource};

# codel00p-providers

Rust inference provider abstraction for codel00p.

The crate gives the rest of the project one interface for inference:

```rust
use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, default_registry,
};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = InferenceClient::builder()
    .registry(default_registry())
    .credential("custom", Credential::api_key("secret"))
    .build();

let response = client
    .complete(
        InferenceRequest::builder("custom", "model-id")
            .base_url("https://provider.example.com/v1")
            .message(ChatMessage::user("Summarize this project."))
            .build(),
    )
    .await?;
# let _ = response;
# Ok(())
# }
```

## Current surface

Implemented:

- provider registry with canonical IDs and aliases;
- initial provider profiles for OpenAI, Anthropic, Azure AI Foundry, AWS
  Bedrock, Google Gemini, GitHub Copilot, GitHub Models, OpenRouter, and custom
  OpenAI-compatible endpoints;
- high-level `InferenceClient` facade;
- inspectable `resolve()` route API and response route metadata with safe audit
  fields for provider, API mode, auth type, base URL source, credential
  source/kind, policy decision, route policy metadata, model catalog URL,
  output-token parameter, and provider capabilities;
- `InferenceClient::list_model_catalog` and `list_models` with
  `ModelCatalogRequest`, normalized `ProviderModel` descriptors, and safe
  catalog metadata for requested/canonical provider, auth type, catalog URL,
  credential source/kind, active model filters, and pre/post-filter model
  counts;
- opt-in fallback routing across provider/model candidates for retryable
  provider failures, with ordered route-attempt metadata, catalog URLs,
  output-token parameters, and capabilities attached to successful responses;
- credential injection by canonical provider or alias;
- organization-managed credential injection with safe `organization:<ref>`
  route source metadata;
- managed-identity credential injection with safe `managed_identity:<ref>`
  route source metadata;
- Azure, AWS, and GCP managed identity credential resolvers with mocked
  metadata-server tests and default-off live smoke tests;
- client-level provider cloud proxy routes with safe route metadata and
  request-level base URL override precedence;
- environment credential loading through `credentials_from_env()`, with safe
  route metadata that records the source variable name instead of secret values;
- provider, model, auth-type, credential-kind, and credential-source-kind
  allowlist policy, provider and catalog capability requirements, and an
  enterprise-direct template plus enterprise cloud-proxy, custom-gateway,
  managed-identity, organization-credential, and direct-agentic templates;
- serde-serializable provider policies with empty/default fields omitted for
  cloud and desktop control-plane defaults;
- OpenAI-compatible Chat Completions transport;
- Azure AI Foundry deployment Chat Completions transport;
- Anthropic Messages transport;
- OpenAI Responses transport;
- AWS Bedrock Converse transport;
- Gemini-native GenerateContent transport;
- normalized responses, usage, cache tokens, reasoning tokens, and tool calls;
- optional request-supplied, client-injected, or published catalog model pricing
  with normalized response cost estimates;
- mocked integration tests for request payloads and response parsing.

Not yet implemented:

- streaming.

The current working transports are enough to use OpenAI Responses, Anthropic
directly, Azure AI Foundry deployment endpoints, AWS Bedrock Converse with
SigV4 credentials, Gemini GenerateContent, custom OpenAI-compatible endpoints,
GitHub Copilot, GitHub Models, OpenRouter, and other compatible gateways.

Azure requests use the resource endpoint as `base_url` and can set deployment
and API version explicitly:

```rust
# use codel00p_providers::{ChatMessage, InferenceRequest};
let request = InferenceRequest::builder("azure", "gpt-4.1")
    .base_url("https://example.openai.azure.com")
    .deployment("team-chat")
    .api_version("2024-10-21")
    .message(ChatMessage::user("Summarize this project."))
    .build();
# let _ = request;
```

If `deployment` is omitted, the request model is used as the deployment name.

Published pricing catalogs can be loaded into a client and reused across
requests:

```rust
# use codel00p_providers::{
#     Credential, InferenceClient, ProviderModelPricing, ProviderPricingCatalog,
#     UsagePricing, default_registry,
# };
let pricing = ProviderPricingCatalog::new([ProviderModelPricing::new(
    "openai",
    "gpt-5-mini",
    UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000),
)])
.with_source("catalog:team-ai-2026-06");

let client = InferenceClient::builder()
    .registry(default_registry())
    .credential("openai", Credential::api_key("secret"))
    .pricing_catalog(pricing)
    .build();
# let _ = client;
```

`UsageCostEstimate.pricing_source` records whether pricing came from the request,
configured client pricing, or a published catalog source such as
`catalog:team-ai-2026-06`.

Clients can also load supported provider credentials from environment variables.
Explicit `credential(...)` calls take precedence over values loaded from the
environment:

```rust
# use codel00p_providers::{InferenceClient, default_registry};
let client = InferenceClient::builder()
    .registry(default_registry())
    .credentials_from_env()
    .build();
# let _ = client;
```

Organization-managed callers can inject credentials with a safe source label for
route audit surfaces:

```rust
# use codel00p_providers::{Credential, InferenceClient, default_registry};
let client = InferenceClient::builder()
    .registry(default_registry())
    .organization_credential(
        "openai",
        Credential::api_key("secret"),
        "team-ai/openai-prod",
    )
    .build();
# let _ = client;
```

Resolved routes report `credential_source` as
`organization:team-ai/openai-prod`, never the credential value.

Managed identity resolvers can inject an already resolved credential with a
typed source kind and safe identity reference:

```rust
# use codel00p_providers::{Credential, InferenceClient, default_registry};
let client = InferenceClient::builder()
    .registry(default_registry())
    .managed_identity_credential(
        "openai",
        Credential::api_key("short-lived-token"),
        "azure/workload-prod",
    )
    .build();
# let _ = client;
```

Resolved routes and model catalogs report both `credential_source` and
`credential_source_kind`, for example `managed_identity:azure/workload-prod`
with `ManagedIdentity`, without exposing token values.

Resolver integrations can also supply the short-lived credential through the
`ManagedIdentityCredentialResolver` boundary:

```rust
# use codel00p_providers::{
#     Credential, InferenceClient, ManagedIdentityCredentialRequest,
#     ManagedIdentityCredentialResolver, ProviderError, default_registry,
# };
struct StaticResolver;

impl ManagedIdentityCredentialResolver for StaticResolver {
    fn resolve(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
    ) -> Result<Credential, ProviderError> {
        assert_eq!(request.provider(), "openai");
        assert_eq!(request.identity_ref(), "azure/workload-prod");
        Ok(Credential::api_key("short-lived-token"))
    }
}

let client = InferenceClient::builder()
    .registry(default_registry())
    .managed_identity_credential_from_resolver(
        "openai",
        "azure/workload-prod",
        &StaticResolver,
    )?
    .build();
# Ok::<(), ProviderError>(())
```

Azure IMDS token acquisition is available through
`AzureManagedIdentityCredentialResolver`. It follows Microsoft's managed
identity endpoint contract and uses a proxy-free client for the default
`http://169.254.169.254/metadata/identity/oauth2/token` endpoint:

```rust
# use codel00p_providers::{
#     AzureManagedIdentityCredentialResolver, InferenceClient, ProviderError, default_registry,
# };
# async fn run() -> Result<(), ProviderError> {
let resolver = AzureManagedIdentityCredentialResolver::user_assigned_client_id(
    "https://cognitiveservices.azure.com/",
    "azure-client-id",
);

let client = InferenceClient::builder()
    .registry(default_registry())
    .azure_managed_identity_credential_from_resolver(
        "azure-foundry",
        "azure/workload-prod",
        &resolver,
    )
    .await?
    .build();
# let _ = client;
# Ok(())
# }
```

The Azure resolver supports system-assigned identities and user-assigned
`client_id`, `object_id`, and `msi_res_id` selectors. See Microsoft's Azure
IMDS managed identity documentation:
<https://learn.microsoft.com/en-us/azure/virtual-machines/instance-metadata-service?tabs=linux#managed-identity>.

AWS EC2 instance profile credential acquisition is available through
`AwsManagedIdentityCredentialResolver`. It uses IMDSv2, resolves the instance
profile role name when one is not configured, and returns `AwsSigV4`
credentials for Bedrock-style providers:

```rust
# use codel00p_providers::{
#     AwsManagedIdentityCredentialResolver, InferenceClient, ProviderError, default_registry,
# };
# async fn run() -> Result<(), ProviderError> {
let resolver = AwsManagedIdentityCredentialResolver::instance_profile("us-east-1")
    .with_role_name("bedrock-prod-role");

let client = InferenceClient::builder()
    .registry(default_registry())
    .aws_managed_identity_credential_from_resolver(
        "bedrock",
        "aws/instance-profile-prod",
        &resolver,
    )
    .await?
    .build();
# let _ = client;
# Ok(())
# }
```

See AWS's IMDSv2 and instance profile credential documentation:
<https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html>
and
<https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/iam-roles-for-amazon-ec2.html>.

GCP metadata server token acquisition is available through
`GcpManagedIdentityCredentialResolver`. It returns the service account OAuth
access token as a bearer-compatible `Credential::ApiKey`, which is suitable for
custom/OpenAI-compatible routes that send `Authorization: Bearer ...`:

```rust
# use codel00p_providers::{
#     GcpManagedIdentityCredentialResolver, InferenceClient, ProviderError, default_registry,
# };
# async fn run() -> Result<(), ProviderError> {
let resolver = GcpManagedIdentityCredentialResolver::default_service_account();

let client = InferenceClient::builder()
    .registry(default_registry())
    .gcp_managed_identity_credential_from_resolver(
        "custom",
        "gcp/default-service-account",
        &resolver,
    )
    .await?
    .build();
# let _ = client;
# Ok(())
# }
```

The native Gemini transport currently uses the Gemini API-key header, so use
this resolver with bearer-compatible routes. See Google Cloud's metadata server
documentation:
<https://cloud.google.com/compute/docs/metadata/querying-metadata>.

This keeps cloud-specific token acquisition outside the route resolver while
preserving the same safe audit metadata.

GitHub has two distinct profiles. Use `github` for the Copilot-compatible
endpoint at `https://api.githubcopilot.com`; it uses `max_completion_tokens`.
Use `github-models` for the official GitHub Models API at
`https://models.github.ai/inference`; it posts to `/inference/chat/completions`,
uses `max_tokens`, and lists models from
`https://models.github.ai/catalog/models`.

## Design rules

- Keep the public API small and ergonomic.
- Keep provider quirks inside profiles and transports.
- Keep route resolution inspectable and safe to log.
- Keep model catalog listing provider-neutral while preserving provider-specific
  fields in `provider_data`.
- Normalize common catalog metadata, common capability flags, and known
  provider-specific annotations into typed fields before policy or UI code needs
  it.
- Enforce policy before inference and reflect route/catalog policy metadata;
  use `list_model_catalog` when a caller needs auditable policy metadata and
  `list_models` when it only needs the filtered model descriptors.
- Use provider-scoped auth type policy when an organization needs to require
  cloud-proxy, API-key, AWS SDK, GitHub Copilot, custom, or other profile auth
  modes for a provider.
- Use provider-scoped credential kind policy when an organization needs to
  require API keys, AWS SigV4 credentials, or intentionally unauthenticated
  catalog access for a provider.
- Use provider-scoped credential source kind policy when an organization needs
  to require managed identity, organization-managed, environment, configured, or
  cloud-proxy credentials for a provider.
- Use provider capability requirements when an organization needs to require
  tool, streaming, vision, or reasoning support before a route or catalog is
  considered allowed.
- Keep route and model catalog audit metadata safe: report auth type,
  credential source/kind/source kind, policy metadata, catalog URL source, and
  counts without exposing credential values.
- Keep `ProviderPolicy` JSON safe and sparse: it contains only policy IDs,
  enums, and capability booleans, not credential values or endpoint secrets.
- Keep policy templates conservative: direct corporate providers can be allowed
  by default while broker and custom endpoints remain explicit choices; use
  `enterprise_cloud_proxy` when those direct providers must resolve through
  codel00p-managed proxy routes, `enterprise_custom_gateway` when inference
  must use the configured OpenAI-compatible gateway profile,
  `enterprise_managed_identity` when direct providers must use managed identity
  credential injection or resolver-backed managed identity credentials,
  `enterprise_organization_credentials` when direct providers must use
  organization-managed credential injection, and `enterprise_direct_agentic`
  when catalog listings should also require tool-use, streaming, and reasoning
  capability flags.
- Never expose credential values in route/debug types.
- Prefer explicit request base URL overrides over configured provider proxies,
  then provider defaults.
- Normalize every provider response into one codel00p response shape.
- Preserve provider usage detail that affects accounting, including cache-read,
  cache-write, and reasoning-token counters when providers expose them.
- Keep cost estimates explicit: callers or organization-managed clients supply
  request pricing, direct model pricing, or published pricing catalogs;
  providers supply usage, and the crate derives deterministic fixed-point
  estimates.
- Prefer request pricing over client and catalog model pricing when both are
  configured for the same provider/model route.
- Preserve provider-specific replay data under `provider_data`, not top-level
  fields.
- Test every transport with mocked HTTP and exact payload assertions.

## Integration Tests

Normal test runs are offline and deterministic:

```bash
cargo test -p codel00p-providers
```

Live provider tests are ignored by default. To run them, enable integration
tests and provide credentials through environment variables:

```bash
CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_GITHUB_TOKEN=... \
cargo test -p codel00p-providers --test live_copilot -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN=... \
CODEL00P_PROVIDER_GITHUB_MODELS_MODEL=openai/gpt-4o-mini \
cargo test -p codel00p-providers --test live_github_models -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_ANTHROPIC_API_KEY=... \
CODEL00P_PROVIDER_ANTHROPIC_MODEL=claude-3-5-haiku-20241022 \
cargo test -p codel00p-providers --test live_anthropic -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_OPENAI_API_KEY=... \
CODEL00P_PROVIDER_OPENAI_MODEL=gpt-5-mini \
cargo test -p codel00p-providers --test live_openai -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY=... \
CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT=https://example.openai.azure.com \
CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT=team-chat \
cargo test -p codel00p-providers --test live_azure_foundry -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID=... \
CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY=... \
CODEL00P_PROVIDER_AWS_REGION=us-east-1 \
CODEL00P_PROVIDER_BEDROCK_MODEL=anthropic.claude-3-5-haiku-20241022-v1:0 \
cargo test -p codel00p-providers --test live_bedrock -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_GEMINI_API_KEY=... \
CODEL00P_PROVIDER_GEMINI_MODEL=gemini-2.5-flash \
cargo test -p codel00p-providers --test live_gemini -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS=1 \
CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE=https://cognitiveservices.azure.com/ \
cargo test -p codel00p-providers --test live_managed_identity -- --ignored live_azure_managed_identity_resolver_smoke_test --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS=1 \
CODEL00P_PROVIDER_AWS_REGION=us-east-1 \
cargo test -p codel00p-providers --test live_managed_identity -- --ignored live_aws_managed_identity_resolver_smoke_test --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS=1 \
cargo test -p codel00p-providers --test live_managed_identity -- --ignored live_gcp_managed_identity_resolver_smoke_test --nocapture
```

Credential environment variables:

| Provider | Variables, in priority order |
| --- | --- |
| GitHub Copilot (`github`) | `CODEL00P_PROVIDER_GITHUB_TOKEN`, `COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN` |
| GitHub Models (`github-models`) | `CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN` |
| OpenRouter | `CODEL00P_PROVIDER_OPENROUTER_API_KEY`, `OPENROUTER_API_KEY` |
| OpenAI | `CODEL00P_PROVIDER_OPENAI_API_KEY`, `OPENAI_API_KEY` |
| Anthropic | `CODEL00P_PROVIDER_ANTHROPIC_API_KEY`, `ANTHROPIC_API_KEY`, `ANTHROPIC_TOKEN` |
| AWS Bedrock | `CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID`, `CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY`, `CODEL00P_PROVIDER_AWS_SESSION_TOKEN`, `CODEL00P_PROVIDER_AWS_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `AWS_REGION`, `AWS_DEFAULT_REGION` |
| Azure AI Foundry | `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY`, `AZURE_FOUNDRY_API_KEY` |
| Gemini | `CODEL00P_PROVIDER_GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GEMINI_API_KEY` |
| Custom endpoint | `CODEL00P_PROVIDER_CUSTOM_API_KEY` |

Azure live tests also need:

| Setting | Variables, in priority order |
| --- | --- |
| Endpoint | `CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT`, `AZURE_FOUNDRY_ENDPOINT`, `AZURE_OPENAI_ENDPOINT` |
| Deployment | `CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT`, `AZURE_FOUNDRY_DEPLOYMENT`, `AZURE_OPENAI_DEPLOYMENT` |
| API version | `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION`, `AZURE_FOUNDRY_API_VERSION`, `AZURE_OPENAI_API_VERSION`; defaults to `2024-10-21` |

Managed identity live smoke tests must run inside the matching cloud runtime
with metadata-server access:

| Cloud | Required opt-in | Optional settings |
| --- | --- | --- |
| Azure | `CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS=1` | `CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE` defaults to `https://cognitiveservices.azure.com/`; use one of `CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_CLIENT_ID`, `CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_OBJECT_ID`, or `CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE_ID` for user-assigned identities |
| AWS | `CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS=1` plus one region variable | `CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_ROLE` can pin the instance profile role name; region uses `CODEL00P_PROVIDER_AWS_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| GCP | `CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS=1` | `CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_SERVICE_ACCOUNT` defaults to `default` |

GitHub Models live tests can set `CODEL00P_PROVIDER_GITHUB_MODELS_MODEL`; it
defaults to `openai/gpt-4o-mini`.

The `CODEL00P_PROVIDER_*` variables are preferred so local integration tests do
not accidentally consume unrelated shell credentials. The fallback variables
match common provider conventions where useful.

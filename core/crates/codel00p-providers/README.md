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
- inspectable `resolve()` route API with safe audit metadata for provider,
  API mode, base URL source, credential presence, policy decision, model catalog
  URL, and provider capabilities;
- `InferenceClient::list_models` with `ModelCatalogRequest` and normalized
  `ProviderModel` descriptors, including descriptions, common capabilities,
  modalities, and token limits for provider setup and future policy;
- opt-in fallback routing across provider/model candidates for retryable
  provider failures, with ordered route-attempt metadata attached to successful
  responses;
- credential injection by canonical provider or alias;
- client-level provider cloud proxy routes with safe route metadata and
  request-level base URL override precedence;
- provider and model allowlist policy, including an enterprise-direct template;
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

- environment/cloud credential resolvers;
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
)]);

let client = InferenceClient::builder()
    .registry(default_registry())
    .credential("openai", Credential::api_key("secret"))
    .pricing_catalog(pricing)
    .build();
# let _ = client;
```

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
- Normalize common catalog metadata such as descriptions, capabilities,
  modalities, and token limits into typed fields before provider-specific
  policy or UI code needs it.
- Enforce policy before inference and reflect model policy in catalog listings.
- Keep policy templates conservative: direct corporate providers can be allowed
  by default while broker and custom endpoints remain explicit choices.
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

GitHub Models live tests can set `CODEL00P_PROVIDER_GITHUB_MODELS_MODEL`; it
defaults to `openai/gpt-4o-mini`.

The `CODEL00P_PROVIDER_*` variables are preferred so local integration tests do
not accidentally consume unrelated shell credentials. The fallback variables
match common provider conventions where useful.

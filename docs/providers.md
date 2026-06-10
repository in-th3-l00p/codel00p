# Inference Providers

`codel00p-providers` is the Rust inference layer for the codel00p ecosystem. It
should give the harness, CLI, desktop app, and cloud platform one consistent way
to route model calls without tying project memory to any single inference
vendor.

This design is based on a detailed reading of Hermes Agent's provider layer.
Hermes models providers as declarative profiles plus transport adapters. That is
the right shape for codel00p, but codel00p should own the Rust contract instead
of depending on Hermes internals.

## Initial supported providers

The first provider set should focus on providers commonly approved in corporate
engineering environments.

| Provider | Why first | Primary API mode | Credential model |
| --- | --- | --- | --- |
| Anthropic | Common Claude deployment for engineering teams | `anthropic_messages` | API key, organization secret, or cloud proxy |
| OpenAI | Common GPT deployment and Responses-style agent support | `responses` and `chat_completions` | API key, organization secret, or cloud proxy |
| Azure AI Foundry | Enterprise Microsoft procurement and Azure governance | `azure_chat_completions` | API key plus customer endpoint and deployment |
| AWS Bedrock | Enterprise AWS procurement and region/governance controls | `bedrock_converse` | AWS SDK credential chain |
| Google Gemini | Google AI Studio and Google Cloud model access | `gemini` and OpenAI-compatible variants | API key, OAuth, or organization secret |
| GitHub Copilot | Common developer-seat entitlement path | `chat_completions` | Copilot-compatible GitHub token |
| GitHub Models | GitHub-native model access for teams and experiments | `chat_completions` | GitHub token or organization secret |
| OpenRouter | Brokered access, experimentation, and broad model fallback | `chat_completions` | API key or cloud proxy |
| Custom OpenAI-compatible | Self-hosted, vendor gateways, Vercel AI Gateway, vLLM, Ollama-compatible endpoints | `chat_completions` | configured endpoint plus optional secret |

The next wave should add the Hermes long-tail only after the core contract is
stable: DeepSeek, NVIDIA NIM, Hugging Face, Alibaba DashScope, Qwen OAuth,
Kimi/Moonshot, xAI, Z.AI/GLM, MiniMax, Novita, Nous, Ollama Cloud, and other
OpenAI-compatible providers.

## Hermes findings

Hermes separates provider support into two layers:

- `ProviderProfile`: identity, aliases, API mode, auth type, environment
  variables, base URLs, model catalog behavior, default auxiliary model, and
  provider-specific request hooks.
- `ProviderTransport`: the wire-protocol adapter for one API mode. It converts
  messages and tools, builds provider request kwargs, normalizes responses, maps
  finish reasons, and extracts provider-specific cache metadata.

Hermes currently uses these important API modes:

- `chat_completions`: OpenAI-compatible chat completions. This covers most
  gateway and model-provider APIs, but provider quirks still matter.
- `anthropic_messages`: Anthropic's Messages API and compatible endpoints.
- `codex_responses`: OpenAI Responses-style calls used for Codex-like agent
  behavior and reasoning item replay.
- `bedrock_converse`: AWS Bedrock Converse through AWS SDK credentials.
- external process modes: Copilot ACP and similar subprocess protocols.
- provider-native exceptions: Gemini can look OpenAI-compatible in config while
  still requiring native thinking configuration and client behavior.

The practical lesson is that provider support cannot be only "send JSON to a
URL". It needs typed provider profiles, protocol-specific transports,
credential resolution, normalized usage, provider data replay, model catalogs,
and policy hooks.

## Rust crate shape

The provider layer should be implemented as a Rust workspace package:

```text
core/crates/codel00p-providers/
  src/
    lib.rs
    error.rs
    registry.rs
    profile.rs
    request.rs
    response.rs
    credentials.rs
    policy.rs
    usage.rs
    transports/
      mod.rs
      chat_completions.rs
      anthropic_messages.rs
      responses.rs
      bedrock_converse.rs
      gemini.rs
      external_process.rs
    providers/
      mod.rs
      anthropic.rs
      openai.rs
      azure_foundry.rs
      bedrock.rs
      gemini.rs
      github.rs
      openrouter.rs
      custom.rs
```

Responsibilities:

- `profile.rs`: provider metadata and hooks represented as Rust traits and
  serializable configuration.
- `registry.rs`: provider lookup by canonical ID, alias, organization policy,
  or configured endpoint.
- `request.rs`: provider-neutral inference request types.
- `response.rs`: normalized model output, tool calls, reasoning, provider data,
  and finish reasons.
- `credentials.rs`: local secret, environment, organization secret, cloud
  proxy, OAuth, AWS SDK, and external-process credential resolution.
- `policy.rs`: organization allowlists, model restrictions, spend ceilings,
  audit fields, and data-residency constraints.
- `usage.rs`: normalized token, cache, reasoning, and cost metadata.
- `transports/*`: protocol-specific request builders and response parsers.
- `providers/*`: built-in profiles for the initial supported providers.

## Core Rust contracts

The first implementation should define these contracts before any provider
becomes feature-rich:

```rust
pub enum ApiMode {
    ChatCompletions,
    AzureChatCompletions,
    AnthropicMessages,
    Responses,
    BedrockConverse,
    Gemini,
    ExternalProcess,
}

pub enum AuthType {
    ApiKey,
    OAuthExternal,
    AwsSdk,
    GitHubCopilot,
    CloudProxy,
    Custom,
}

pub struct ProviderProfile {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub description: &'static str,
    pub api_mode: ApiMode,
    pub auth_type: AuthType,
    pub env_vars: &'static [&'static str],
    pub default_base_url: Option<&'static str>,
    pub models_url: Option<&'static str>,
    pub default_aux_model: Option<&'static str>,
    pub capabilities: ProviderCapabilities,
}

#[async_trait::async_trait]
pub trait ProviderTransport: Send + Sync {
    fn api_mode(&self) -> ApiMode;
    async fn build_request(&self, request: InferenceRequest) -> Result<WireRequest, ProviderError>;
    async fn parse_response(&self, response: WireResponse) -> Result<InferenceResponse, ProviderError>;
}
```

The real structs will need owned and borrowed variants where useful, but the
contract should stay this small at first.

## Request flow

```text
Harness
  |
  | InferenceRequest
  v
Provider registry
  |
  | resolve profile by provider id, alias, model, endpoint, or org policy
  v
Credential resolver
  |
  | env, user secret, organization secret, cloud proxy, AWS SDK, OAuth, external process
  v
Policy engine
  |
  | model allowlist, budget, workspace restrictions, audit tags
  v
Transport
  |
  | protocol-specific request and response normalization
  v
InferenceResponse
  |
  | content, tool calls, reasoning, usage, provider data
  v
Harness + memory layer
```

Project memory must not store provider-specific credentials or assume a
provider. It can store useful provider-neutral facts, such as "this project has
a large generated-client directory; avoid loading it unless needed." Provider
data needed for reasoning replay should stay in session state, not durable
project memory, unless explicitly promoted into reviewed knowledge.

## Provider-specific notes

### Anthropic

Use a native `anthropic_messages` transport. Convert codel00p messages into
Anthropic system plus message blocks, convert tools into input schemas, preserve
thinking blocks where available, map stop reasons into codel00p finish reasons,
and extract cache read/write token metadata.

### OpenAI

Support both Chat Completions and Responses. Chat Completions is required for
OpenAI-compatible gateways. Responses is required for richer agent behavior,
reasoning item replay, and Codex-like execution. The response parser must retain
provider data needed for subsequent turns without leaking it into project
memory.

### Azure AI Foundry

Treat Azure as a deployment-aware Chat Completions provider, not a plain
OpenAI-compatible clone. Requests use the customer resource endpoint as
`base_url`, then post to:

```text
/openai/deployments/{deployment}/chat/completions?api-version={api_version}
```

The request can set `deployment` and `api_version`; if deployment is omitted,
the request model is used as the deployment name. This keeps enterprise
configuration explicit: resource-specific endpoints, organization policy,
separate model deployment names, and auditable request routing.

### AWS Bedrock

Use a separate `bedrock_converse` transport. It should use the AWS SDK
credential chain, region-aware clients, Converse message conversion, tool config
conversion, guardrail configuration, and Bedrock stop-reason normalization.

### Google Gemini

Do not hide Gemini behind a generic OpenAI-compatible assumption. Gemini needs a
native transport for thinking configuration, multimodal payloads, and
provider-specific response shape. OpenAI-compatible Gemini endpoints can be
supported through `chat_completions`, but they should still use Gemini-aware
profile hooks.

### GitHub Copilot

Support GitHub as a corporate developer-seat path, but keep it behind a clear
credential boundary. The current `github` provider profile targets
`https://api.githubcopilot.com` through the Chat Completions-compatible
transport and uses `max_completion_tokens`, matching the Copilot-style request
shape already covered by mocked tests.

### GitHub Models

Treat official GitHub Models as a separate provider profile instead of an alias
for Copilot. The `github-models` profile targets
`https://models.github.ai/inference`, posts chat requests to
`/inference/chat/completions`, uses `max_tokens`, and lists catalog entries from
`https://models.github.ai/catalog/models`. Catalog parsing accepts GitHub's
top-level model array and normalizes `publisher` into `owned_by` while
preserving GitHub-specific metadata in `provider_data`.

### OpenRouter

Support OpenRouter as a broker and experimentation path. Preserve provider
preferences, routing metadata, reasoning configuration, and public model catalog
fetching. Organization policy must be able to restrict which downstream models
can be used through OpenRouter.

### Custom OpenAI-compatible

Support configured endpoints for enterprise gateways, self-hosted models, and
developer-managed model servers. The first version should support base URL,
optional API key, model ID, timeout, custom headers, and health-check behavior.

## Implementation phases

1. Define Rust contracts: `ApiMode`, `AuthType`, `ProviderProfile`,
   `ProviderCapabilities`, `InferenceRequest`, `InferenceResponse`,
   `ProviderTransport`, and `ProviderError`.
2. Implement static built-in profiles for the initial provider set.
3. Implement registry lookup by provider ID and alias.
4. Implement `chat_completions` transport first because it unlocks Azure,
   OpenRouter, custom endpoints, and many future providers.
5. Implement credential resolution with explicit source tracking.
6. Implement `anthropic_messages`.
7. Implement `responses`.
8. Implement `bedrock_converse`.
9. Implement Gemini-native GenerateContent support.
10. Add organization policy enforcement and cloud proxy routing.
11. Add model catalog fetching and fallback model lists.
12. Add normalized usage and audit metadata.
13. Add conformance tests using captured request/response fixtures for every
    supported API mode.

## Current implementation status

The first implementation now lives in `core/crates/codel00p-providers`.

Implemented:

- Rust workspace and `codel00p-providers` crate;
- `InferenceClient` facade;
- `InferenceRequest` and `InferenceResponse`;
- chat messages and function tool definitions;
- normalized tool calls and token usage;
- `ProviderProfile`, `ProviderRegistry`, `ApiMode`, and `AuthType`;
- first-wave provider profiles and aliases, including separate `github`
  Copilot and `github-models` GitHub Models profiles;
- inspectable `ResolvedInferenceRoute` with safe audit metadata for provider,
  API mode, base URL source, credential presence, policy decision, model catalog
  URL, and provider capabilities;
- client-level provider cloud proxy routing, including proxy credential use,
  request-level base URL override precedence, and safe `CloudProxy` route
  metadata;
- model catalog listing through `ModelCatalogRequest`,
  `InferenceClient::list_models`, and normalized `ProviderModel` descriptors
  with common capabilities, modalities, and token limits;
- opt-in fallback routing with ordered route-attempt metadata and conservative
  fallback only for classified retryable/provider-unavailable failures;
- provider and model allowlist policy, including catalog filtering for
  disallowed models and an enterprise-direct provider template;
- OpenAI-compatible Chat Completions transport with mocked HTTP tests;
- GitHub Models profile coverage for `models.github.ai/inference`, `max_tokens`,
  and top-level array model catalogs;
- opt-in GitHub Models live smoke-test coverage using the official
  `github-models` profile and model override;
- typed catalog metadata for provider capabilities, supported modalities, and
  token limits while preserving raw provider fields in `provider_data`;
- `ProviderPolicy::enterprise_direct()` for organizations that want direct
  first-wave providers while leaving broker/custom endpoints as explicit opt-ins;
- client-level provider/model pricing injection with request-level pricing
  taking precedence for deterministic cost estimates;
- Azure AI Foundry deployment Chat Completions transport with mocked HTTP
  tests for deployment URLs, API version query parameters, `api-key` auth,
  omitted request model fields, default deployment behavior, and missing
  credentials;
- opt-in Azure AI Foundry live smoke-test configuration for customer resource
  endpoints, deployment names, API versions, and API key credentials;
- Anthropic Messages transport with mocked HTTP tests, including native system
  prompts, tool schemas, `tool_use` responses, tool-result replay, stop
  reasons, and cache usage metadata;
- OpenAI Responses transport with mocked HTTP tests, including stateless
  requests, system/developer/user messages, function tools, function-call
  replay, text/tool-call normalization, provider replay data, and usage
  metadata;
- AWS Bedrock Converse transport with mocked HTTP tests, including SigV4 request
  signing, system prompts, message blocks, tool specs, `toolUse` responses,
  `toolResult` replay, stop reasons, and cache usage metadata;
- Gemini GenerateContent transport with mocked HTTP tests, including
  `systemInstruction`, content parts, function declarations, `functionCall`
  responses, `functionResponse` replay, finish reasons, and usage metadata.
- request-supplied `UsagePricing` and response-level `UsageCostEstimate`
  metadata using deterministic nano-unit fixed-point math.
- opt-in live integration test configuration using `CODEL00P_INTEGRATION_TESTS`
  and provider-specific credential environment variables.

Next provider work should focus on enterprise variants: cloud-managed pricing
publication and richer provider-specific catalog metadata where common typed
fields are not enough.

## Non-goals for the first pass

- Porting Hermes' Python implementation directly.
- Supporting every Hermes provider immediately.
- Coupling provider state to durable project memory.
- Building a plugin ABI before the built-in Rust contract is proven.
- Optimizing cost routing before correctness, policy, and observability work.

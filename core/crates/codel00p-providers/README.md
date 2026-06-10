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
  Bedrock, Google Gemini, GitHub Models, OpenRouter, and custom
  OpenAI-compatible endpoints;
- high-level `InferenceClient` facade;
- inspectable `resolve()` route API;
- credential injection by canonical provider or alias;
- provider allowlist policy;
- OpenAI-compatible Chat Completions transport;
- Anthropic Messages transport;
- OpenAI Responses transport;
- AWS Bedrock Converse transport;
- Gemini-native GenerateContent transport;
- normalized responses, usage, and tool calls;
- mocked integration tests for request payloads and response parsing.

Not yet implemented:

- environment/cloud credential resolvers;
- streaming.

The current working transports are enough to use OpenAI Responses, Anthropic
directly, AWS Bedrock Converse with SigV4 credentials, Gemini GenerateContent,
custom OpenAI-compatible endpoints, OpenRouter, Azure-style endpoints when a
base URL is supplied, and other compatible gateways.

## Design rules

- Keep the public API small and ergonomic.
- Keep provider quirks inside profiles and transports.
- Keep route resolution inspectable and safe to log.
- Never expose credential values in route/debug types.
- Normalize every provider response into one codel00p response shape.
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
CODEL00P_PROVIDER_ANTHROPIC_API_KEY=... \
CODEL00P_PROVIDER_ANTHROPIC_MODEL=claude-3-5-haiku-20241022 \
cargo test -p codel00p-providers --test live_anthropic -- --ignored --nocapture

CODEL00P_INTEGRATION_TESTS=1 \
CODEL00P_PROVIDER_OPENAI_API_KEY=... \
CODEL00P_PROVIDER_OPENAI_MODEL=gpt-5-mini \
cargo test -p codel00p-providers --test live_openai -- --ignored --nocapture

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
| GitHub Copilot / GitHub Models | `CODEL00P_PROVIDER_GITHUB_TOKEN`, `COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN` |
| OpenRouter | `CODEL00P_PROVIDER_OPENROUTER_API_KEY`, `OPENROUTER_API_KEY` |
| OpenAI | `CODEL00P_PROVIDER_OPENAI_API_KEY`, `OPENAI_API_KEY` |
| Anthropic | `CODEL00P_PROVIDER_ANTHROPIC_API_KEY`, `ANTHROPIC_API_KEY`, `ANTHROPIC_TOKEN` |
| AWS Bedrock | `CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID`, `CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY`, `CODEL00P_PROVIDER_AWS_SESSION_TOKEN`, `CODEL00P_PROVIDER_AWS_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `AWS_REGION`, `AWS_DEFAULT_REGION` |
| Azure AI Foundry | `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY`, `AZURE_FOUNDRY_API_KEY` |
| Gemini | `CODEL00P_PROVIDER_GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GEMINI_API_KEY` |
| Custom endpoint | `CODEL00P_PROVIDER_CUSTOM_API_KEY` |

The `CODEL00P_PROVIDER_*` variables are preferred so local integration tests do
not accidentally consume unrelated shell credentials. The fallback variables
match common provider conventions where useful.

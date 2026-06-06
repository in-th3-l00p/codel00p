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
- normalized responses, usage, and tool calls;
- mocked integration tests for request payloads and response parsing.

Not yet implemented:

- Anthropic Messages transport;
- OpenAI Responses transport;
- AWS Bedrock Converse transport;
- Gemini-native transport;
- environment/cloud credential resolvers;
- streaming.

The current working transport is enough to use custom OpenAI-compatible
endpoints, OpenRouter, Azure-style endpoints when a base URL is supplied, and
other compatible gateways.

## Design rules

- Keep the public API small and ergonomic.
- Keep provider quirks inside profiles and transports.
- Keep route resolution inspectable and safe to log.
- Never expose credential values in route/debug types.
- Normalize every provider response into one codel00p response shape.
- Preserve provider-specific replay data under `provider_data`, not top-level
  fields.
- Test every transport with mocked HTTP and exact payload assertions.


//! Friendly, rustc-style explanations for common provider/inference failures.
//!
//! Provider transports surface raw HTTP errors (`status 404 ... No endpoints
//! found that support tool use`, `status 429 ...`, ...). Those are accurate but
//! unfriendly. [`explain_provider_error`] recognises the common ones, assigns a
//! stable `CLxxxx` code, and renders an actionable message with a link to the
//! troubleshooting docs — the way `rustc` points at `--explain`.

/// Base URL of the troubleshooting reference; each code is an anchor on it.
const DOCS_URL: &str =
    "https://github.com/in-th3-l00p/codel00p/blob/main/docs/troubleshooting/provider-errors.md";

/// A recognised failure mode, with what it means and how to fix it.
struct Explanation {
    code: &'static str,
    title: String,
    detail: &'static str,
    actions: Vec<String>,
}

/// Turn a raw inference/provider error into a friendly, actionable message.
///
/// Returns `None` when the error isn't one we recognise, so callers can fall
/// back to the raw text rather than hide an unexpected failure.
pub fn explain_provider_error(raw: &str, provider: &str, model: &str) -> Option<String> {
    let lower = raw.to_lowercase();

    let explanation = if lower.contains("support tool use") {
        Explanation {
            code: "CL0001",
            title: format!("the model `{model}` can't be used — it doesn't support tool use"),
            detail: "codel00p's agent works by calling tools (reading files, running \
                     commands, ...), so it needs a chat model with a tool-capable endpoint. \
                     Rerank, embedding, and some vision-only models don't qualify and the \
                     provider rejects the request.",
            actions: vec![
                format!(
                    "pick a tool-capable model:  codel00p config providers use {provider} --model <model>"
                ),
                "switch model for later turns from inside chat:  /model <model>".to_string(),
                "browse your provider's tool-capable models (on OpenRouter, filter by \"Tools\")"
                    .to_string(),
            ],
        }
    } else if lower.contains("maximum context length")
        || lower.contains("context_length_exceeded")
        || lower.contains("context length is")
    {
        Explanation {
            code: "CL0002",
            title: "the conversation is bigger than the model's context window".to_string(),
            detail: "The request exceeded the model's maximum context length — usually a long \
                     conversation or a large tool output (e.g. reading a big file).",
            actions: vec![
                "start a fresh chat — a bare `codel00p` always begins a new session".to_string(),
                "inside chat, /reset starts a new conversation".to_string(),
                format!(
                    "use a larger-context model:  codel00p config providers use {provider} --model <model>"
                ),
            ],
        }
    } else if lower.contains("too many requests")
        || lower.contains("rate-limit")
        || lower.contains("rate limit")
        || lower.contains("status 429")
    {
        Explanation {
            code: "CL0003",
            title: format!("`{provider}` is rate-limiting `{model}` right now"),
            detail: "The provider temporarily refused the request (HTTP 429). Free and shared \
                     models hit this often when busy.",
            actions: vec![
                "wait a few seconds and send the message again".to_string(),
                "add your own provider API key to get higher, dedicated limits".to_string(),
                format!(
                    "switch to a less-busy model:  codel00p config providers use {provider} --model <model>"
                ),
            ],
        }
    } else if lower.contains("status 401")
        || lower.contains("unauthorized")
        || lower.contains("no auth credentials")
        || lower.contains("invalid api key")
    {
        Explanation {
            code: "CL0004",
            title: format!("`{provider}` rejected your credentials"),
            detail: "The provider returned an authentication error (HTTP 401): the API key is \
                     missing, wrong, or lacks access to this model.",
            actions: vec![
                format!("set or update the key:  codel00p config providers set-key {provider}"),
                format!("check which key is used:  codel00p config providers show {provider}"),
            ],
        }
    } else if (lower.contains("status 404") || lower.contains("not found"))
        && lower.contains("model")
    {
        Explanation {
            code: "CL0005",
            title: format!("`{provider}` doesn't recognise the model `{model}`"),
            detail: "The provider returned 'model not found' — the model id is misspelled, \
                     retired, or not available on your account.",
            actions: vec![
                format!("see the configured model:  codel00p config providers show {provider}"),
                format!(
                    "set a valid model:  codel00p config providers use {provider} --model <model>"
                ),
            ],
        }
    } else {
        return None;
    };

    Some(render(&explanation))
}

fn render(explanation: &Explanation) -> String {
    let mut out = format!("error[{}]: {}\n\n", explanation.code, explanation.title);
    out.push_str("  what happened\n");
    out.push_str(&format!("    {}\n\n", explanation.detail));
    out.push_str("  to fix it, try one of\n");
    for action in &explanation.actions {
        out.push_str(&format!("    • {action}\n"));
    }
    out.push_str(&format!(
        "\n  for more information, see\n    {DOCS_URL}#{}\n",
        explanation.code.to_lowercase()
    ));
    out
}

/// Friendly explanation if recognised, otherwise the original error unchanged.
pub fn humanize_provider_error(raw: &str, provider: &str, model: &str) -> String {
    explain_provider_error(raw, provider, model).unwrap_or_else(|| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tool_use_unsupported() {
        let raw = "inference failed: http request to provider `openrouter` failed: status 404 \
                   Not Found: {\"error\":{\"message\":\"No endpoints found that support tool use.\"}}";
        let msg =
            explain_provider_error(raw, "openrouter", "nvidia/rerank:free").expect("explained");
        assert!(msg.contains("error[CL0001]"), "{msg}");
        assert!(msg.contains("nvidia/rerank:free"), "{msg}");
        assert!(msg.contains("/model <model>"), "{msg}");
        assert!(msg.contains("provider-errors.md#cl0001"), "{msg}");
    }

    #[test]
    fn classifies_context_overflow() {
        let raw = "inference failed: ... maximum context length is 131072 tokens. However ...";
        let msg = explain_provider_error(raw, "openrouter", "m").expect("explained");
        assert!(msg.contains("error[CL0002]"), "{msg}");
    }

    #[test]
    fn classifies_rate_limit() {
        let raw = "inference failed: ... status 429 Too Many Requests ...";
        let msg = explain_provider_error(raw, "openrouter", "m").expect("explained");
        assert!(msg.contains("error[CL0003]"), "{msg}");
    }

    #[test]
    fn classifies_auth_failure() {
        let raw = "inference failed: ... status 401 Unauthorized: no auth credentials found";
        let msg = explain_provider_error(raw, "openai", "gpt").expect("explained");
        assert!(msg.contains("error[CL0004]"), "{msg}");
        assert!(msg.contains("set-key openai"), "{msg}");
    }

    #[test]
    fn unknown_errors_are_left_alone() {
        let raw = "inference failed: connection reset by peer";
        assert!(explain_provider_error(raw, "openrouter", "m").is_none());
        assert_eq!(humanize_provider_error(raw, "openrouter", "m"), raw);
    }
}

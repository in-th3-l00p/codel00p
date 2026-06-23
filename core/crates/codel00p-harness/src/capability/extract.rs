//! Auto-extracting a capability from a completed turn.
//!
//! Given the turn's executed tool calls (and, for the model-backed extractor, the
//! model itself), decide whether the work is worth saving as a reusable
//! capability and shape it. The pure helpers (success detection, step selection,
//! slug/title derivation, JSON parsing) sit beside the two `CapabilityExtractor`
//! implementations they support. The `Capability` model and the tools live in
//! `super`.

use super::*;

/// One executed tool call from a completed turn, as a capability extractor sees
/// it: the tool name, the arguments the model passed, and the result content.
#[derive(Clone, Debug)]
pub struct CapabilityCandidateCall {
    pub name: String,
    pub input: Value,
    pub output: Value,
}

/// What a [`CapabilityExtractor`] inspects at turn end to decide whether the work
/// is worth freezing into a reusable capability.
#[derive(Clone, Debug)]
pub struct CapabilityExtractionRequest {
    pub goal: String,
    pub assistant_message: Option<String>,
    pub calls: Vec<CapabilityCandidateCall>,
}

/// Proposes a capability from a completed turn, or `None` if the work was not
/// capability-worthy. Returning a candidate sends it to the review queue.
#[async_trait]
pub trait CapabilityExtractor: Send + Sync {
    async fn extract(
        &self,
        request: CapabilityExtractionRequest,
    ) -> Result<Option<Capability>, HarnessError>;
}

/// Did a `run_pipeline` result indicate every step succeeded?
fn pipeline_succeeded(output: &Value) -> bool {
    let total = output.get("total").and_then(Value::as_u64).unwrap_or(0);
    let completed = output.get("completed").and_then(Value::as_u64).unwrap_or(0);
    let stopped = output
        .get("stopped_early")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    total > 0 && completed == total && !stopped
}

/// The steps of the most recent fully-successful `run_pipeline` call this turn.
fn successful_pipeline_steps(calls: &[CapabilityCandidateCall]) -> Option<Vec<Value>> {
    calls
        .iter()
        .rev()
        .find(|call| call.name == "run_pipeline" && pipeline_succeeded(&call.output))
        .and_then(|call| call.input.get("steps").and_then(Value::as_array).cloned())
}

/// A tool-name slug usable as a capability name: lowercase, `_`-separated,
/// starting with a letter.
fn capability_slug(goal: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in goal.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    match trimmed.chars().next() {
        Some(c) if c.is_ascii_lowercase() => trimmed,
        Some(_) => format!("cap_{trimmed}"),
        None => String::new(),
    }
}

fn first_line(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() > 120 {
        format!("{}…", line.chars().take(119).collect::<String>())
    } else {
        line.to_string()
    }
}

/// Deterministic extractor: when a turn ran a fully-successful multi-step
/// `run_pipeline`, freeze that pipeline verbatim into a (zero-parameter)
/// capability candidate named after the goal. No extra inference; it captures
/// the exact successful program for a human (or the model extractor) to
/// generalize and approve.
#[derive(Clone, Copy, Debug, Default)]
pub struct PipelineCapabilityExtractor;

#[async_trait]
impl CapabilityExtractor for PipelineCapabilityExtractor {
    async fn extract(
        &self,
        request: CapabilityExtractionRequest,
    ) -> Result<Option<Capability>, HarnessError> {
        if request
            .assistant_message
            .as_deref()
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .is_none()
        {
            return Ok(None);
        }
        let Some(steps) = successful_pipeline_steps(&request.calls) else {
            return Ok(None);
        };
        if steps.len() < 2 {
            return Ok(None);
        }
        let name = capability_slug(&request.goal);
        if name.is_empty() {
            return Ok(None);
        }

        let capability = Capability {
            name,
            description: first_line(&request.goal),
            parameters: empty_object_schema(),
            steps,
        };
        // Only propose if it is structurally valid.
        if capability.validate().is_err() {
            return Ok(None);
        }
        Ok(Some(capability))
    }
}

/// LLM-assisted extractor: asks a model to *generalize* a successful pipeline
/// into a parameterized, reusable capability (a name, a description, a parameter
/// schema, and templated steps referencing `{{params.<name>}}`). This is the
/// "tools that write tools" path; the model lifts concrete literals into
/// parameters so the capability is reusable, not a one-off freeze. Any failure
/// (no candidate pipeline, bad JSON, invalid shape) yields `None` so extraction
/// never disrupts the turn.
pub struct ModelCapabilityExtractor {
    model_client: Arc<dyn crate::turn::ModelClient>,
}

impl ModelCapabilityExtractor {
    pub fn new(model_client: Arc<dyn crate::turn::ModelClient>) -> Self {
        Self { model_client }
    }
}

#[async_trait]
impl CapabilityExtractor for ModelCapabilityExtractor {
    async fn extract(
        &self,
        request: CapabilityExtractionRequest,
    ) -> Result<Option<Capability>, HarnessError> {
        let Some(steps) = successful_pipeline_steps(&request.calls) else {
            return Ok(None);
        };
        let steps_json = serde_json::to_string_pretty(&steps).unwrap_or_default();
        let prompt = format!(
            "You turn a successful tool pipeline into a reusable, parameterized \
             capability for a coding agent.\n\nThe user's request was:\n{goal}\n\nThe \
             agent ran this pipeline successfully (steps as JSON):\n{steps_json}\n\n\
             Generalize it into a capability by lifting concrete literals (file \
             names, identifiers, messages) into named parameters. Reply with ONLY a \
             JSON object of this exact shape:\n{{\n  \"name\": \
             \"snake_case_tool_name\",\n  \"description\": \"one line, what it does\",\n  \
             \"parameters\": {{ \"type\": \"object\", \"required\": [...], \
             \"properties\": {{ \"<param>\": {{ \"type\": \"string\" }} }} }},\n  \
             \"steps\": [ {{ \"tool\": \"...\", \"input\": {{ ... }} }} ]\n}}\nIn \
             step inputs, reference a parameter as {{{{params.<name>}}}} and an \
             earlier step's output as {{{{steps.N.field}}}}. Keep the same tools \
             and order as the pipeline above. Output JSON only, no prose.",
            goal = request.goal,
        );

        let mut session = crate::session::SessionState::new(
            crate::session::SessionId::from_static("capability-extraction"),
        );
        session.push_user(crate::session::UserMessage::new(prompt));
        let inference = crate::turn::HarnessInferenceRequest::new(session)
            .with_response_format(crate::turn::ResponseFormat::JsonObject);

        let response = self.model_client.infer(inference).await?;
        let Some(message) = response.assistant_message() else {
            return Ok(None);
        };
        let Some(capability) = parse_capability_json(message) else {
            return Ok(None);
        };
        if capability.validate().is_err() {
            return Ok(None);
        }
        Ok(Some(capability))
    }
}

/// Parse a `Capability` out of a model reply, tolerating ```json fences and
/// surrounding prose by extracting the outermost JSON object.
pub(super) fn parse_capability_json(message: &str) -> Option<Capability> {
    let start = message.find('{')?;
    let end = message.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<Capability>(&message[start..=end]).ok()
}

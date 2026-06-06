use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    HarnessError, HarnessInferenceRequest, HarnessInferenceResponse, ModelClient,
};

#[derive(Clone, Default)]
pub struct ScriptedModelClient {
    requests: Arc<Mutex<Vec<HarnessInferenceRequest>>>,
    responses: Arc<Mutex<Vec<HarnessInferenceResponse>>>,
}

impl ScriptedModelClient {
    pub fn new(responses: Vec<HarnessInferenceResponse>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into_iter().rev().collect())),
        }
    }

    pub fn requests(&self) -> Vec<HarnessInferenceRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

#[async_trait]
impl ModelClient for ScriptedModelClient {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        self.requests.lock().expect("requests lock").push(request);
        self.responses
            .lock()
            .expect("responses lock")
            .pop()
            .ok_or_else(|| HarnessError::InferenceFailed {
                message: "scripted model client has no response".to_string(),
            })
    }
}

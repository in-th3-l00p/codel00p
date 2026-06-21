//! Project-memory retrieval and candidate persistence for CLI agent turns.

use super::*;

pub(super) struct CliProjectMemoryProvider {
    config: CliConfig,
    limit: Option<usize>,
    proactive: bool,
}

impl CliProjectMemoryProvider {
    pub(super) fn new(config: CliConfig) -> Self {
        Self {
            config,
            limit: None,
            proactive: true,
        }
    }

    pub(super) fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Toggles proactive task-aware recall (default on). When off, the turn's
    /// task text is ignored and memory is retrieved by filters only.
    pub(super) fn with_proactive(mut self, proactive: bool) -> Self {
        self.proactive = proactive;
        self
    }
}

#[async_trait]
impl ProjectMemoryProvider for CliProjectMemoryProvider {
    async fn retrieve(
        &self,
        request: ProjectMemoryRequest,
    ) -> Result<ProjectMemoryContext, codel00p_harness::HarnessError> {
        let store = open_memory_store(&self.config)
            .map_err(|message| codel00p_harness::HarnessError::InferenceFailed { message })?;

        // Proactive task-aware recall: when enabled and the turn carries task
        // text, rank approved memory against the current goal with BM25 so the
        // most relevant memories surface automatically. Otherwise fall back to
        // deterministic-order retrieval (prior behavior).
        let task = if self.proactive {
            request.latest_user_message()
        } else {
            None
        };

        let items = if let Some(task) = task {
            let mut query = MemoryRetrievalQuery::new(self.config.project.clone(), task);
            if let Some(limit) = self.limit {
                query = query.with_limit(limit);
            }
            store
                .retrieve_ranked(query)
                .map_err(|error| codel00p_harness::HarnessError::InferenceFailed {
                    message: error.to_string(),
                })?
                .into_iter()
                .map(|memory| {
                    ProjectMemoryItem::new(
                        memory.entry().id(),
                        memory.entry().kind(),
                        memory.entry().content(),
                        memory.entry().tags().to_vec(),
                        format!("proactive recall (relevance {})", memory.score()),
                    )
                })
                .collect()
        } else {
            let mut query = MemoryQuery::new(self.config.project.clone());
            if let Some(limit) = self.limit {
                query = query.with_limit(limit);
            }
            store
                .retrieve(query)
                .map_err(|error| codel00p_harness::HarnessError::InferenceFailed {
                    message: error.to_string(),
                })?
                .into_iter()
                .map(|memory| {
                    ProjectMemoryItem::new(
                        memory.entry().id(),
                        memory.entry().kind(),
                        memory.entry().content(),
                        memory.entry().tags().to_vec(),
                        memory.reason(),
                    )
                })
                .collect()
        };

        Ok(ProjectMemoryContext::new(items))
    }
}

pub(super) struct CliMemoryCandidateSink {
    config: CliConfig,
}

impl CliMemoryCandidateSink {
    pub(super) fn new(config: CliConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MemoryCandidateSink for CliMemoryCandidateSink {
    async fn persist(
        &self,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<MemoryCandidateSinkOutcome, codel00p_harness::HarnessError> {
        let mut store = open_memory_store(&self.config)
            .map_err(|message| codel00p_harness::HarnessError::InferenceFailed { message })?;
        let mut created_ids = Vec::new();
        let mut duplicate_ids = Vec::new();

        for candidate in candidates {
            let id = candidate.id().to_string();
            match store.create_candidate(candidate) {
                Ok(_) => created_ids.push(id),
                Err(
                    MemoryError::MemoryAlreadyExists { .. } | MemoryError::DuplicateMemory { .. },
                ) => duplicate_ids.push(id),
                Err(error) => {
                    return Err(codel00p_harness::HarnessError::InferenceFailed {
                        message: error.to_string(),
                    });
                }
            }
        }

        Ok(MemoryCandidateSinkOutcome::from_parts(
            created_ids,
            duplicate_ids,
        ))
    }
}

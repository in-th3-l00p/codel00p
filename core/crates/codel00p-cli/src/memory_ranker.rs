//! The external memory ranking provider — the CLI's implementation of the
//! [`RankingProvider`] seam from `codel00p-memory`.
//!
//! By default memory relevance is ranked with offline BM25 in-process. When an
//! operator opts into an external ranker (governance-gated in
//! [`crate::config::resolve_cli_config`]), the store is built with an
//! [`ExternalRanker`] instead: it POSTs the query and candidate memory content
//! to a configured service and maps the returned scores back onto the corpus.
//!
//! Resilience is a first-class property here. A ranking-service hiccup must never
//! break memory retrieval, so every failure path — connection error, non-2xx
//! status, malformed body — degrades gracefully to the same offline BM25 the
//! default provider uses. The external ranker can only *improve* ordering; it can
//! never take retrieval down.

use std::collections::HashMap;
use std::time::Duration;

use codel00p_memory::{Bm25RankingProvider, MemoryError, RankingDocument, RankingProvider};
use serde::{Deserialize, Serialize};

/// Connect/read timeout for a ranking request. A slow service should not stall a
/// turn; on timeout we fall back to BM25.
const RANK_TIMEOUT: Duration = Duration::from_secs(10);

/// A [`RankingProvider`] backed by a remote ranking service. Falls back to
/// offline BM25 on any failure, so retrieval is never worse than the default.
pub struct ExternalRanker {
    url: String,
    http: reqwest::blocking::Client,
    fallback: Bm25RankingProvider,
}

impl ExternalRanker {
    /// Build a ranker that POSTs to `url`. A client-build failure is non-fatal —
    /// it leaves a default client, and request failures fall back to BM25 anyway.
    pub fn new(url: String) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(RANK_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            url,
            http,
            fallback: Bm25RankingProvider,
        }
    }

    /// Attempt a ranking request. Returns scores aligned by input index, or an
    /// error describing why the service could not be used (so the caller can log
    /// it before falling back). Documents the service omits score 0.
    fn try_rank(&self, query: &str, documents: &[RankingDocument]) -> Result<Vec<u8>, String> {
        let request = RankRequest {
            query,
            documents: documents
                .iter()
                .map(|document| RankRequestDocument {
                    id: &document.id,
                    content: &document.content,
                })
                .collect(),
        };
        let response = self
            .http
            .post(&self.url)
            .json(&request)
            .send()
            .map_err(|error| format!("request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("service returned status {}", response.status()));
        }
        let parsed: RankResponse = response
            .json()
            .map_err(|error| format!("invalid response body: {error}"))?;

        // Key by document id so the service may return scores in any order (or
        // omit some). Clamp into the 0..=100 contract; missing documents score 0.
        let by_id: HashMap<&str, u8> = parsed
            .scores
            .iter()
            .map(|entry| (entry.id.as_str(), entry.score.min(100) as u8))
            .collect();
        Ok(documents
            .iter()
            .map(|document| by_id.get(document.id.as_str()).copied().unwrap_or(0))
            .collect())
    }
}

impl RankingProvider for ExternalRanker {
    fn rank(&self, query: &str, documents: &[RankingDocument]) -> Result<Vec<u8>, MemoryError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        match self.try_rank(query, documents) {
            Ok(scores) => Ok(scores),
            Err(error) => {
                // Graceful degradation: never let a ranking-service problem break
                // retrieval. Note it and rank with offline BM25 instead.
                eprintln!(
                    "codel00p: external ranking failed ({error}); falling back to offline BM25"
                );
                self.fallback.rank(query, documents)
            }
        }
    }
}

/// Request body POSTed to the ranking service.
#[derive(Serialize)]
struct RankRequest<'a> {
    query: &'a str,
    documents: Vec<RankRequestDocument<'a>>,
}

#[derive(Serialize)]
struct RankRequestDocument<'a> {
    id: &'a str,
    content: &'a str,
}

/// Response body expected from the ranking service: a relevance score per
/// document id. Scores are clamped into `0..=100`; ids the service omits are
/// treated as score 0.
#[derive(Deserialize)]
struct RankResponse {
    scores: Vec<RankResponseScore>,
}

#[derive(Deserialize)]
struct RankResponseScore {
    id: String,
    score: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn documents() -> Vec<RankingDocument> {
        vec![
            RankingDocument::new("mem-a", "deploy the service to the kubernetes cluster"),
            RankingDocument::new("mem-b", "unrelated note about release timing"),
        ]
    }

    #[test]
    fn uses_service_scores_aligned_by_id() {
        let server = MockServer::start();
        // The service returns scores out of input order and keyed by id; the
        // ranker must realign them to the input order (mem-a, mem-b).
        let mock = server.mock(|when, then| {
            when.method(POST).path("/rank");
            then.status(200).json_body(serde_json::json!({
                "scores": [
                    { "id": "mem-b", "score": 12 },
                    { "id": "mem-a", "score": 95 },
                ]
            }));
        });

        let ranker = ExternalRanker::new(format!("{}/rank", server.base_url()));
        let scores = ranker.rank("deploy to kubernetes", &documents()).unwrap();

        mock.assert();
        assert_eq!(scores, vec![95, 12]);
    }

    #[test]
    fn clamps_scores_and_defaults_missing_ids_to_zero() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/rank");
            // mem-a over the cap (clamped to 100); mem-b omitted (defaults to 0).
            then.status(200).json_body(serde_json::json!({
                "scores": [ { "id": "mem-a", "score": 250 } ]
            }));
        });

        let ranker = ExternalRanker::new(format!("{}/rank", server.base_url()));
        let scores = ranker.rank("deploy to kubernetes", &documents()).unwrap();

        mock.assert();
        assert_eq!(scores, vec![100, 0]);
    }

    #[test]
    fn falls_back_to_bm25_on_server_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/rank");
            then.status(500);
        });

        let docs = documents();
        let ranker = ExternalRanker::new(format!("{}/rank", server.base_url()));
        let scores = ranker.rank("deploy to kubernetes", &docs).unwrap();

        // Identical to what offline BM25 would have produced.
        let expected = Bm25RankingProvider
            .rank("deploy to kubernetes", &docs)
            .unwrap();
        assert_eq!(scores, expected);
    }

    #[test]
    fn falls_back_to_bm25_on_malformed_body() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/rank");
            then.status(200).body("not json");
        });

        let docs = documents();
        let ranker = ExternalRanker::new(format!("{}/rank", server.base_url()));
        let scores = ranker.rank("deploy to kubernetes", &docs).unwrap();

        let expected = Bm25RankingProvider
            .rank("deploy to kubernetes", &docs)
            .unwrap();
        assert_eq!(scores, expected);
    }

    #[test]
    fn empty_corpus_short_circuits_without_a_request() {
        // No documents → no request is made; an unreachable URL must not matter.
        let ranker = ExternalRanker::new("http://127.0.0.1:1/rank".to_string());
        let scores = ranker.rank("anything", &[]).unwrap();
        assert!(scores.is_empty());
    }
}

//! Offline, deterministic ranking and near-duplicate similarity for memory.
//!
//! Two scorers live here, both pure-Rust and fully offline (no embeddings, no
//! network):
//!
//! * [`Bm25Ranker`] — a BM25-lite relevance ranker used to order a candidate set
//!   against a free-text query. BM25 rewards memories that contain the query's
//!   terms, saturates the contribution of any single repeated term, and weighs
//!   rare terms (high inverse-document-frequency) more than common ones.
//! * [`shingle_similarity`] — token n-gram (shingle) Jaccard similarity used for
//!   near-duplicate detection. Comparing ordered n-grams (rather than bag-of-words
//!   overlap) catches reworded duplicates that share phrasing while staying
//!   robust to small edits.
//!
//! The [`MemoryRanker`] trait is the extension seam: the default implementation
//! is [`Bm25Ranker`], and a future vector/embedding backend can implement the
//! same trait without touching the repository. Keeping the seam here (not a
//! vector implementation) is deliberate — the property we preserve is "offline,
//! deterministic, auditable".

use std::collections::{BTreeMap, BTreeSet};

/// BM25 term-frequency saturation parameter. Standard default.
const BM25_K1: f64 = 1.2;
/// BM25 document-length normalization parameter. Standard default.
const BM25_B: f64 = 0.75;

/// Default shingle width for near-duplicate detection (token bigrams).
const SHINGLE_N: usize = 2;

/// Minimal English stopword set. Dropping these keeps BM25 focused on
/// content-bearing terms; the list is intentionally small and fixed so ranking
/// stays deterministic and predictable.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of", "on",
    "or", "that", "the", "to", "with",
];

/// Tokenizes text into lowercase alphanumeric terms, preserving order and
/// duplicates (BM25 needs term frequencies), with stopwords removed.
pub(crate) fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .collect()
}

/// A candidate document for ranking: an opaque caller-chosen key plus the
/// document's tokenized content.
#[derive(Clone, Debug)]
pub struct RankCandidate<K> {
    pub key: K,
    pub tokens: Vec<String>,
}

impl<K> RankCandidate<K> {
    pub fn new(key: K, tokens: Vec<String>) -> Self {
        Self { key, tokens }
    }
}

/// A scored candidate: the caller's key plus a 0..=100 relevance score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankedCandidate<K> {
    pub key: K,
    pub score: u8,
}

/// Ranks a candidate set against query terms, returning a 0..=100 score per
/// candidate. The default implementation is [`Bm25Ranker`]; a future embedding
/// backend implements this same trait so the repository never has to change.
///
/// Implementations must be deterministic: the same inputs always produce the
/// same scores in the same order.
pub trait MemoryRanker {
    fn rank<K: Clone>(
        &self,
        query_terms: &[String],
        candidates: &[RankCandidate<K>],
    ) -> Vec<RankedCandidate<K>>;
}

/// BM25-lite lexical ranker. Computes inverse-document-frequency over the
/// supplied candidate set (the candidates *are* the corpus) and scores each
/// candidate's BM25 sum against the query terms, then maps the raw BM25 score
/// onto a 0..=100 scale.
#[derive(Clone, Debug, Default)]
pub struct Bm25Ranker;

impl MemoryRanker for Bm25Ranker {
    fn rank<K: Clone>(
        &self,
        query_terms: &[String],
        candidates: &[RankCandidate<K>],
    ) -> Vec<RankedCandidate<K>> {
        if candidates.is_empty() {
            return Vec::new();
        }

        let n = candidates.len() as f64;
        let avg_len: f64 = if candidates.is_empty() {
            0.0
        } else {
            candidates
                .iter()
                .map(|candidate| candidate.tokens.len() as f64)
                .sum::<f64>()
                / n
        };

        // Document frequency per query term across the candidate corpus.
        let query_set: BTreeSet<&String> = query_terms.iter().collect();
        let mut doc_freq: BTreeMap<&String, usize> = BTreeMap::new();
        for term in &query_set {
            let count = candidates
                .iter()
                .filter(|candidate| candidate.tokens.contains(term))
                .count();
            doc_freq.insert(term, count);
        }

        // Raw BM25 score per candidate.
        let raw: Vec<f64> = candidates
            .iter()
            .map(|candidate| bm25_score(query_terms, &candidate.tokens, avg_len, n, &doc_freq))
            .collect();

        // Map raw scores onto 0..=100. We scale by the maximum *achievable* score
        // for this query/corpus (a hypothetical document that contains every
        // query term once at the average length), so a near-perfect match lands
        // near 100 and the threshold semantics stay meaningful. A candidate that
        // shares no query term scores a raw 0 → mapped 0.
        let max_possible = max_possible_bm25(query_terms, avg_len, n, &doc_freq);

        candidates
            .iter()
            .zip(raw)
            .map(|(candidate, raw_score)| {
                let score = if max_possible <= 0.0 || raw_score <= 0.0 {
                    0
                } else {
                    ((raw_score / max_possible) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                };
                RankedCandidate {
                    key: candidate.key.clone(),
                    score,
                }
            })
            .collect()
    }
}

fn idf(doc_freq: usize, corpus_size: f64) -> f64 {
    // BM25 idf with the standard +0.5 smoothing, floored at a small positive
    // value so a term appearing in *every* candidate still contributes a little
    // (rather than going negative and penalizing matches).
    let df = doc_freq as f64;
    let value = ((corpus_size - df + 0.5) / (df + 0.5) + 1.0).ln();
    value.max(1e-6)
}

fn bm25_score(
    query_terms: &[String],
    doc_tokens: &[String],
    avg_len: f64,
    corpus_size: f64,
    doc_freq: &BTreeMap<&String, usize>,
) -> f64 {
    if doc_tokens.is_empty() {
        return 0.0;
    }
    let doc_len = doc_tokens.len() as f64;

    // Distinct query terms — a repeated query term should not double-count its
    // idf; BM25 saturates per-term via tf already.
    let mut score = 0.0;
    for term in query_terms.iter().collect::<BTreeSet<_>>() {
        let tf = doc_tokens.iter().filter(|token| *token == term).count() as f64;
        if tf == 0.0 {
            continue;
        }
        let df = doc_freq.get(&term).copied().unwrap_or(0);
        let term_idf = idf(df, corpus_size);
        let numerator = tf * (BM25_K1 + 1.0);
        let denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (doc_len / avg_len.max(1.0)));
        score += term_idf * (numerator / denominator);
    }
    score
}

/// Largest BM25 score achievable for this query against a hypothetical document
/// of average length that contains every distinct query term exactly once. Used
/// as the denominator when mapping raw scores onto 0..=100.
fn max_possible_bm25(
    query_terms: &[String],
    _avg_len: f64,
    corpus_size: f64,
    doc_freq: &BTreeMap<&String, usize>,
) -> f64 {
    let mut score = 0.0;
    for term in query_terms.iter().collect::<BTreeSet<_>>() {
        let df = doc_freq.get(&term).copied().unwrap_or(0);
        // A term that matched no candidate cannot lift any candidate; skip it so
        // an off-corpus query word doesn't deflate everyone's mapped score.
        if df == 0 {
            continue;
        }
        let term_idf = idf(df, corpus_size);
        let tf = 1.0;
        let numerator = tf * (BM25_K1 + 1.0);
        // Average-length doc → length normalization factor is exactly 1.
        let denominator = tf + BM25_K1;
        score += term_idf * (numerator / denominator);
    }
    score
}

/// Builds the ordered set of token n-grams (shingles) for a token sequence. With
/// fewer than `n` tokens the whole sequence is treated as a single shingle so
/// short contents still compare meaningfully.
fn shingles(tokens: &[String], n: usize) -> BTreeSet<String> {
    if tokens.is_empty() {
        return BTreeSet::new();
    }
    if tokens.len() < n {
        return BTreeSet::from([tokens.join(" ")]);
    }
    tokens.windows(n).map(|window| window.join(" ")).collect()
}

/// Near-duplicate similarity between two contents as a 0..=100 score, using
/// token n-gram (shingle) Jaccard. Comparing bigrams instead of bag-of-words
/// catches reworded duplicates that share phrasing — overlap that unigram or
/// substring matching misses — while staying deterministic and offline.
pub(crate) fn shingle_similarity(left: &str, right: &str) -> u8 {
    let left_shingles = shingles(&tokenize(left), SHINGLE_N);
    let right_shingles = shingles(&tokenize(right), SHINGLE_N);
    if left_shingles.is_empty() || right_shingles.is_empty() {
        return 0;
    }
    let intersection = left_shingles.intersection(&right_shingles).count();
    let union = left_shingles.union(&right_shingles).count();
    if union == 0 {
        return 0;
    }
    (((intersection * 100) + (union / 2)) / union) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(key: &str, content: &str) -> RankCandidate<String> {
        RankCandidate::new(key.to_string(), tokenize(content))
    }

    #[test]
    fn bm25_ranks_topical_match_above_keyword_overlap_but_irrelevant() {
        // "build" appears in both, but the relevant memory shares more of the
        // distinctive query terms, so BM25 must rank it first.
        let candidates = vec![
            candidate("relevant", "run cargo build then cargo test before pushing"),
            candidate("irrelevant", "build a friendly relationship with the team"),
        ];
        let query = tokenize("how do I build and test the cargo project");
        let ranked = Bm25Ranker.rank(&query, &candidates);

        let relevant = ranked.iter().find(|r| r.key == "relevant").unwrap();
        let irrelevant = ranked.iter().find(|r| r.key == "irrelevant").unwrap();
        assert!(
            relevant.score > irrelevant.score,
            "relevant {} should outrank irrelevant {}",
            relevant.score,
            irrelevant.score
        );
    }

    #[test]
    fn bm25_weighs_rare_terms_more_than_common_ones() {
        // "cargo" appears in every candidate (common, low idf); "kubernetes"
        // appears in one (rare, high idf). A query for both should rank the rare
        // match highest.
        let candidates = vec![
            candidate("rare", "deploy with cargo to kubernetes cluster"),
            candidate("common-a", "cargo build the workspace"),
            candidate("common-b", "cargo test the workspace"),
        ];
        let query = tokenize("cargo kubernetes");
        let ranked = Bm25Ranker.rank(&query, &candidates);

        let rare = ranked.iter().find(|r| r.key == "rare").unwrap();
        let common = ranked.iter().find(|r| r.key == "common-a").unwrap();
        assert!(
            rare.score > common.score,
            "rare-term match {} should beat common-term-only match {}",
            rare.score,
            common.score
        );
    }

    #[test]
    fn bm25_is_deterministic() {
        let candidates = vec![
            candidate("a", "run cargo build before pushing main"),
            candidate("b", "configure the dashboard widget"),
        ];
        let query = tokenize("cargo build main");
        let first = Bm25Ranker.rank(&query, &candidates);
        let second = Bm25Ranker.rank(&query, &candidates);
        assert_eq!(first, second);
    }

    #[test]
    fn bm25_scores_zero_for_no_shared_terms() {
        let candidates = vec![candidate("a", "configure the dashboard widget")];
        let query = tokenize("cargo build kubernetes");
        let ranked = Bm25Ranker.rank(&query, &candidates);
        assert_eq!(ranked[0].score, 0);
    }

    #[test]
    fn shingle_similarity_catches_reworded_duplicate() {
        // Same meaning, reworded order — bigram Jaccard should still score high.
        let left = "always run cargo fmt and cargo clippy before committing code";
        let right = "before committing code always run cargo clippy and cargo fmt";
        let score = shingle_similarity(left, right);
        assert!(score >= 40, "reworded duplicate scored only {score}");
    }

    #[test]
    fn shingle_similarity_low_for_unrelated() {
        let left = "run cargo fmt before committing";
        let right = "the colorful unicorn dashboard widget";
        assert_eq!(shingle_similarity(left, right), 0);
    }
}

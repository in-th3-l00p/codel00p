//! Offline, deterministic text similarity primitives shared across crates.
//!
//! This is a dependency-light leaf crate (pure `std`, no embeddings, no network)
//! holding the tokenizer and the token n-gram (shingle) Jaccard similarity used
//! for near-duplicate detection. It is consumed by `codel00p-memory` (memory
//! ranking + near-duplicate detection) and `codel00p-skill` (skill consolidation)
//! so both share one auditable, deterministic notion of "how similar are these
//! two texts".
//!
//! The property preserved here is "offline, deterministic, auditable": the same
//! inputs always produce the same score.

use std::collections::BTreeSet;

/// Default shingle width for near-duplicate detection (token bigrams). Comparing
/// ordered bigrams (rather than bag-of-words overlap) catches reworded
/// duplicates that share phrasing while staying robust to small edits.
const SHINGLE_N: usize = 2;

/// Minimal English stopword set. Dropping these keeps similarity focused on
/// content-bearing terms; the list is intentionally small and fixed so scoring
/// stays deterministic and predictable.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of", "on",
    "or", "that", "the", "to", "with",
];

/// Tokenizes text into lowercase alphanumeric terms, preserving order and
/// duplicates (term frequencies matter to BM25), with stopwords removed.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .collect()
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

/// Near-duplicate similarity between two contents as a 0..=100 score, using token
/// n-gram (shingle) Jaccard. Comparing bigrams instead of bag-of-words catches
/// reworded duplicates that share phrasing — overlap that unigram or substring
/// matching misses — while staying deterministic and offline.
pub fn shingle_similarity(left: &str, right: &str) -> u8 {
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

    #[test]
    fn shingle_similarity_identical_is_full() {
        let text = "run cargo build then cargo test before pushing";
        assert_eq!(shingle_similarity(text, text), 100);
    }

    #[test]
    fn tokenize_drops_stopwords_and_lowercases() {
        assert_eq!(
            tokenize("The Cargo BUILD and test"),
            vec!["cargo", "build", "test"]
        );
    }
}

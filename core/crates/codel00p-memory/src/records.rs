//! Memory records, retrieval result wrappers, and deterministic quality scoring.

use std::collections::BTreeSet;

use codel00p_protocol::MemoryEntry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    entry: MemoryEntry,
}

impl MemoryRecord {
    pub(crate) fn new(entry: MemoryEntry) -> Self {
        Self { entry }
    }

    pub fn entry(&self) -> &MemoryEntry {
        &self.entry
    }

    /// Returns deterministic advisory quality signals for review workflows.
    pub fn quality(&self) -> MemoryQuality {
        score_memory_entry(&self.entry)
    }
}

/// Advisory quality score for a memory record.
///
/// Quality findings help review surfaces prioritize cleanup, but they do not
/// change lifecycle state, retrieval eligibility, or duplicate detection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuality {
    pub(crate) score: u8,
    findings: Vec<String>,
}

impl MemoryQuality {
    /// A deterministic score from 0 to 100, where higher is more reusable.
    pub fn score(&self) -> u8 {
        self.score
    }

    /// Stable human-readable findings explaining score deductions.
    pub fn findings(&self) -> &[String] {
        &self.findings
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrievedMemory {
    pub(crate) record: MemoryRecord,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimilarMemory {
    pub(crate) record: MemoryRecord,
    pub(crate) score: u8,
}

/// An approved memory matched by free-text retrieval, carrying its lexical
/// similarity score against the query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankedMemory {
    pub(crate) record: MemoryRecord,
    pub(crate) score: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleMemory {
    pub(crate) record: MemoryRecord,
    pub(crate) newer_record: MemoryRecord,
    pub(crate) score: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityMemory {
    pub(crate) record: MemoryRecord,
    pub(crate) quality: MemoryQuality,
}

impl SimilarMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn score(&self) -> u8 {
        self.score
    }
}

impl RankedMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn score(&self) -> u8 {
        self.score
    }
}

impl StaleMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn newer_entry(&self) -> &MemoryEntry {
        self.newer_record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn newer_quality(&self) -> MemoryQuality {
        self.newer_record.quality()
    }

    pub fn score(&self) -> u8 {
        self.score
    }
}

impl RetrievedMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl QualityMemory {
    /// Returns the low-quality memory entry selected for review.
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    /// Returns the advisory score and findings that matched the query.
    pub fn quality(&self) -> &MemoryQuality {
        &self.quality
    }
}

pub(crate) fn content_tokens(content: &str) -> BTreeSet<String> {
    content
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub(crate) fn token_similarity_score(left: &BTreeSet<String>, right: &BTreeSet<String>) -> u8 {
    if left.is_empty() || right.is_empty() {
        return 0;
    }

    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    (((intersection * 100) + (union / 2)) / union) as u8
}

pub(crate) fn score_memory_entry(entry: &MemoryEntry) -> MemoryQuality {
    let mut score = 100_i16;
    let mut findings = Vec::new();
    let tokens = content_tokens(entry.content());

    if tokens.len() < 8 {
        score -= 25;
        findings.push("content is too short to be reusable".to_string());
    }

    if entry.content().split_whitespace().count() > 80 {
        score -= 15;
        findings.push("content may be too long for frequent retrieval".to_string());
    }

    if contains_vague_language(&tokens) {
        score -= 10;
        findings.push("content uses vague language".to_string());
    }

    MemoryQuality {
        score: score.clamp(0, 100) as u8,
        findings,
    }
}

fn contains_vague_language(tokens: &BTreeSet<String>) -> bool {
    ["important", "stuff", "thing", "things", "this", "that"]
        .iter()
        .any(|token| tokens.contains(*token))
}

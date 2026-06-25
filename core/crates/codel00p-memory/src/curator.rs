//! Deterministic, offline consolidation planning for near-duplicate memories.
//!
//! The curator does not mutate anything: it *plans* consolidations over a set of
//! memory records and returns them for review. A consolidation is a cluster of
//! near-duplicate memories (same kind, shingle similarity at or above a
//! threshold) reduced to a single **survivor** plus the **duplicates** that would
//! be archived if the plan is applied.
//!
//! Because detection is the offline shingle/Jaccard similarity (no LLM), a
//! consolidation never synthesizes new merged text — it keeps the best existing
//! memory and proposes archiving (never deleting) the rest. Survivor selection is
//! deterministic: highest advisory [`crate::MemoryQuality`] score wins, ties broken
//! by the lexicographically smallest id, so the same input always plans the same
//! consolidations.

use std::collections::BTreeMap;

use codel00p_protocol::MemoryEntry;

use crate::{MemoryRecord, ranking::shingle_similarity};

/// Default similarity threshold (0..=100) at or above which two same-kind
/// memories are treated as near-duplicates worth consolidating. Conservative
/// because consolidation archives the duplicates on apply; lower it to surface
/// looser matches for review.
pub const DEFAULT_CONSOLIDATION_THRESHOLD: u8 = 60;

/// A planned consolidation of a near-duplicate cluster: one surviving memory and
/// the duplicates that would be archived as superseded by it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Consolidation {
    survivor: MemoryRecord,
    duplicates: Vec<DuplicateMemory>,
}

/// A memory the curator would archive in favor of a cluster's survivor, carrying
/// its similarity (0..=100) to that survivor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateMemory {
    record: MemoryRecord,
    similarity: u8,
}

impl Consolidation {
    /// The memory kept active for this cluster (highest quality; ties → smallest id).
    pub fn survivor(&self) -> &MemoryRecord {
        &self.survivor
    }

    /// The near-duplicates that would be archived, ordered by id.
    pub fn duplicates(&self) -> &[DuplicateMemory] {
        &self.duplicates
    }
}

impl DuplicateMemory {
    pub fn record(&self) -> &MemoryRecord {
        &self.record
    }

    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    /// Similarity (0..=100) of this duplicate to the cluster's survivor.
    pub fn similarity(&self) -> u8 {
        self.similarity
    }
}

/// Plans consolidations over `records`, treating two records as near-duplicates
/// when they share a [`codel00p_protocol::MemoryKind`] and their shingle
/// similarity is at least `threshold`. Memories of different kinds are never
/// merged. Only clusters of two or more are returned; singletons are left alone.
///
/// Pure and deterministic: callers pass the records to consider (e.g. the active
/// approved memories for a project) and apply the plan separately via the
/// repository's review/archive path.
pub fn plan_consolidations(records: &[MemoryRecord], threshold: u8) -> Vec<Consolidation> {
    // Cluster within each kind independently — cross-kind look-alikes are not
    // duplicates. BTreeMap keeps kind iteration order stable.
    let mut by_kind: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let kind_key = format!("{:?}", record.entry().kind());
        by_kind.entry(kind_key).or_default().push(index);
    }

    let mut consolidations = Vec::new();
    for (_kind, mut indices) in by_kind {
        // Sort by id so union-find iteration (and thus clustering) is stable.
        indices.sort_by(|left, right| records[*left].entry().id().cmp(records[*right].entry().id()));

        let count = indices.len();
        let mut parent: Vec<usize> = (0..count).collect();
        for left in 0..count {
            for right in (left + 1)..count {
                let similarity = shingle_similarity(
                    records[indices[left]].entry().content(),
                    records[indices[right]].entry().content(),
                );
                if similarity >= threshold {
                    union(&mut parent, left, right);
                }
            }
        }

        // Group the global record indices by cluster root, preserving id order.
        let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (position, &record_index) in indices.iter().enumerate() {
            let root = find(&mut parent, position);
            clusters.entry(root).or_default().push(record_index);
        }

        for (_root, mut members) in clusters {
            if members.len() < 2 {
                continue;
            }
            // Survivor: highest advisory quality, ties broken by smallest id.
            members.sort_by(|left, right| {
                records[*right]
                    .quality()
                    .score()
                    .cmp(&records[*left].quality().score())
                    .then_with(|| records[*left].entry().id().cmp(records[*right].entry().id()))
            });
            let survivor_index = members[0];
            let survivor = records[survivor_index].clone();

            let mut duplicates: Vec<DuplicateMemory> = members[1..]
                .iter()
                .map(|&member| DuplicateMemory {
                    record: records[member].clone(),
                    similarity: shingle_similarity(
                        survivor.entry().content(),
                        records[member].entry().content(),
                    ),
                })
                .collect();
            duplicates.sort_by(|left, right| left.entry().id().cmp(right.entry().id()));

            consolidations.push(Consolidation {
                survivor,
                duplicates,
            });
        }
    }

    // Stable output ordering by survivor id.
    consolidations.sort_by(|left, right| left.survivor.entry().id().cmp(right.survivor.entry().id()));
    consolidations
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root == right_root {
        return;
    }
    // Attach the higher-indexed root under the lower for determinism.
    if left_root < right_root {
        parent[right_root] = left_root;
    } else {
        parent[left_root] = right_root;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_protocol::{MemoryEntry, MemoryKind, MemoryStatus, ProjectRef};

    fn record(id: &str, kind: MemoryKind, content: &str) -> MemoryRecord {
        let entry = MemoryEntry::new(
            id.to_string(),
            ProjectRef::new("org", "proj"),
            kind,
            content.to_string(),
        )
        .with_status(MemoryStatus::Approved);
        MemoryRecord::new(entry)
    }

    #[test]
    fn clusters_reworded_duplicates_and_keeps_higher_quality_survivor() {
        let records = vec![
            // Same rule, but vague language ("important"/"thing") → lower quality.
            record(
                "m-vague",
                MemoryKind::Convention,
                "always run the important fmt and clippy thing before committing code changes",
            ),
            // Cleaner phrasing of the same rule → higher quality, should survive.
            record(
                "m-clean",
                MemoryKind::Convention,
                "always run fmt and clippy before committing code changes to the project",
            ),
        ];
        let plan = plan_consolidations(&records, 40);
        assert_eq!(plan.len(), 1, "the two near-dups form one cluster");
        let consolidation = &plan[0];
        assert_eq!(consolidation.survivor().entry().id(), "m-clean");
        assert_eq!(consolidation.duplicates().len(), 1);
        assert_eq!(consolidation.duplicates()[0].entry().id(), "m-vague");
        assert!(consolidation.duplicates()[0].similarity() >= 40);
    }

    #[test]
    fn distinct_phrasing_below_threshold_is_left_alone() {
        // Real near-dups in meaning but the bigram overlap is low → not merged.
        let records = vec![
            record("a", MemoryKind::Convention, "run fmt and clippy"),
            record(
                "b",
                MemoryKind::Convention,
                "always run cargo fmt and cargo clippy before committing",
            ),
        ];
        assert!(plan_consolidations(&records, 60).is_empty());
    }

    #[test]
    fn different_kinds_never_merge() {
        let records = vec![
            record(
                "a",
                MemoryKind::Convention,
                "always run cargo fmt and clippy before committing code",
            ),
            record(
                "b",
                MemoryKind::Workflow,
                "always run cargo fmt and clippy before committing code",
            ),
        ];
        assert!(plan_consolidations(&records, 40).is_empty());
    }

    #[test]
    fn unrelated_memories_produce_no_plan() {
        let records = vec![
            record("a", MemoryKind::Convention, "run cargo fmt before committing"),
            record("b", MemoryKind::Convention, "the colorful unicorn dashboard widget"),
        ];
        assert!(plan_consolidations(&records, 60).is_empty());
    }

    #[test]
    fn is_deterministic() {
        let records = vec![
            record(
                "z",
                MemoryKind::Decision,
                "use postgres as our main primary datastore",
            ),
            record(
                "y",
                MemoryKind::Decision,
                "use postgres for our main primary datastore",
            ),
        ];
        let first = plan_consolidations(&records, 40);
        let second = plan_consolidations(&records, 40);
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        // Equal quality → tie-break by smallest id makes "y" the stable survivor.
        assert_eq!(first[0].survivor().entry().id(), "y");
        assert_eq!(first[0].duplicates()[0].entry().id(), "z");
    }
}

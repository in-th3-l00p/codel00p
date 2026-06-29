//! Description-based agent routing (initiative #13, phase 4).
//!
//! Given a task, rank the registered agents by how well their identity
//! (name + description + persona) matches the task, using the same offline,
//! deterministic BM25 ranker the memory system uses (`codel00p-memory`). The
//! top-scoring agent is the specialist to route the task to. Pure and offline —
//! no LLM, no network — so routing is explainable and reproducible.
//!
//! This is the routing primitive: [`rank_agents`] is reused by the `agent route`
//! CLI command and is the seam a delegation/fan-out layer would call to pick a
//! specialist for a sub-task.

use std::path::Path;

use codel00p_memory::{Bm25Ranker, MemoryRanker, RankCandidate};
use codel00p_textsim::tokenize;

use super::registry::AgentInfo;

/// How much of an agent's `persona.md` is folded into its routing text. Keeps the
/// candidate corpus bounded while letting a rich persona inform routing.
const PERSONA_ROUTING_CHARS: usize = 2000;

/// A scored routing match for one agent (0..=100, higher is a better fit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteMatch {
    pub name: String,
    pub description: Option<String>,
    pub score: u8,
}

/// Rank `agents` against `task`, best match first (ties broken by agent name for
/// determinism). Agents are scored by BM25 over their name + description +
/// persona text, with the agent set as the corpus. An empty task or empty agent
/// set yields an empty ranking. Scores are 0..=100; a score of 0 means the agent
/// shares no content-bearing term with the task.
pub fn rank_agents(agents: &[AgentInfo], task: &str) -> Vec<RouteMatch> {
    if agents.is_empty() {
        return Vec::new();
    }
    let query = tokenize(task);
    let candidates: Vec<RankCandidate<usize>> = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| RankCandidate::new(index, tokenize(&routing_text(agent))))
        .collect();

    let mut matches: Vec<RouteMatch> = Bm25Ranker
        .rank(&query, &candidates)
        .into_iter()
        .map(|ranked| {
            let agent = &agents[ranked.key];
            RouteMatch {
                name: agent.name.clone(),
                description: agent.description.clone(),
                score: ranked.score,
            }
        })
        .collect();

    // BM25 returns candidate order; sort best-first with a deterministic tie-break.
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
    matches
}

/// The best routing match whose score clears `min_score` (i.e. it shares at least
/// some content with the task). Returns `None` when nothing matches, so a caller
/// can fall back to the default agent rather than route to an irrelevant one.
pub fn best_match(agents: &[AgentInfo], task: &str, min_score: u8) -> Option<RouteMatch> {
    rank_agents(agents, task)
        .into_iter()
        .find(|candidate| candidate.score >= min_score.max(1))
}

/// The text used to represent an agent for routing: its name, description, and a
/// bounded slice of its `persona.md` (if present and non-empty).
fn routing_text(agent: &AgentInfo) -> String {
    let mut text = agent.name.clone();
    if let Some(description) = &agent.description {
        text.push(' ');
        text.push_str(description);
    }
    if let Some(persona) = read_persona(&agent.home) {
        text.push(' ');
        let slice: String = persona.chars().take(PERSONA_ROUTING_CHARS).collect();
        text.push_str(&slice);
    }
    text
}

fn read_persona(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join("persona.md"))
        .ok()
        .filter(|persona| !persona.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn agent(name: &str, description: &str) -> AgentInfo {
        AgentInfo {
            name: name.to_string(),
            description: Some(description.to_string()),
            created_at: 0,
            // A nonexistent home → no persona is folded in (description-only ranking).
            home: PathBuf::from("/nonexistent"),
        }
    }

    #[test]
    fn ranks_the_topical_specialist_first() {
        let agents = vec![
            agent("coder", "implements features and refactors rust code"),
            agent(
                "reviewer",
                "reviews pull requests for correctness and style",
            ),
            agent(
                "devops",
                "manages kubernetes deployments and cloud infrastructure",
            ),
        ];
        let ranked = rank_agents(&agents, "refactor the rust code in the parser");
        assert_eq!(ranked[0].name, "coder", "ranking: {ranked:?}");
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn best_match_picks_deployment_specialist() {
        let agents = vec![
            agent("coder", "implements features and refactors rust code"),
            agent(
                "devops",
                "manages kubernetes deployments and cloud infrastructure",
            ),
        ];
        let best = best_match(&agents, "deploy the service to the kubernetes cluster", 1).unwrap();
        assert_eq!(best.name, "devops");
    }

    #[test]
    fn best_match_is_none_when_nothing_overlaps() {
        let agents = vec![agent("devops", "manages kubernetes deployments")];
        // Task shares no content-bearing term → no confident route.
        assert!(best_match(&agents, "write a haiku about the ocean", 1).is_none());
    }

    #[test]
    fn empty_agents_yields_empty_ranking() {
        assert!(rank_agents(&[], "anything").is_empty());
    }

    #[test]
    fn deterministic_and_tie_broken_by_name() {
        // Two agents with identical descriptions tie on score → name order wins.
        let agents = vec![
            agent("zeta", "writes documentation and guides"),
            agent("alpha", "writes documentation and guides"),
        ];
        let first = rank_agents(&agents, "write documentation");
        let second = rank_agents(&agents, "write documentation");
        assert_eq!(first, second);
        assert_eq!(
            first[0].name, "alpha",
            "tie should break to the smaller name"
        );
    }
}

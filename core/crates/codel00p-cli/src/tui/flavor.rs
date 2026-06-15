//! Personality content for the chat TUI, inspired by the Hermes agent's rotating
//! "verbs", long-run "charms", and composer placeholders. Pure and deterministic
//! (cycled by the UI tick / seeded by the session id), so it is unit-testable.

/// Generic thinking words shown in the status spinner while the agent is between
/// tool calls.
const THINKING_VERBS: &[&str] = &[
    "thinking",
    "pondering",
    "reasoning",
    "synthesizing",
    "considering",
    "planning",
    "analyzing",
    "composing",
];

/// Encouraging notes appended once a turn has been running for a while.
const CHARMS: &[&str] = &[
    "still working…",
    "almost there…",
    "untangling things…",
    "double-checking…",
];

/// Rotating empty-composer hints.
const PLACEHOLDERS: &[&str] = &[
    "Ask me anything — or /help for commands",
    "Try \"explain this codebase\"",
    "Try \"write a test for the session store\"",
    "Try \"run the tests and fix what breaks\"",
    "Try \"/continue\" to resume your last chat",
    "Try \"refactor this module\"",
    "Try \"what changed on main recently?\"",
];

/// How many ticks (~120ms each) a verb stays before rotating.
const VERB_TICKS: u64 = 12;

/// The verb to show while a tool is running, mapping codel00p tool names to a
/// present-progressive label; unknown tools fall back to their own name.
pub(crate) fn tool_verb(tool: &str) -> &str {
    match tool {
        "read_file" => "reading",
        "list_files" => "listing",
        "search_text" => "searching",
        "apply_patch" => "editing",
        "create_file" => "creating",
        "update_file" => "writing",
        "delete_file" => "deleting",
        "run_command" => "running",
        "git_status" | "git_diff" | "git_log" => "inspecting",
        "git_commit" => "committing",
        "web_fetch" => "fetching",
        "web_search" => "searching",
        "delegate_task" => "delegating",
        "propose_skill" => "learning",
        other => other,
    }
}

/// The generic thinking verb for the current tick (rotates slowly).
pub(crate) fn thinking_verb(tick: u64) -> &'static str {
    THINKING_VERBS[((tick / VERB_TICKS) as usize) % THINKING_VERBS.len()]
}

/// A long-run charm for the current tick, or `None` early in a turn. `elapsed_ticks`
/// is how long the turn has been running.
pub(crate) fn charm(tick: u64, elapsed_ticks: u64) -> Option<&'static str> {
    // ~25 ticks ≈ 3s before the first charm appears.
    if elapsed_ticks < 25 {
        return None;
    }
    Some(CHARMS[((tick / VERB_TICKS) as usize) % CHARMS.len()])
}

/// A stable composer placeholder chosen from the session id, so each conversation
/// gets a consistent hint.
pub(crate) fn placeholder(seed: &str) -> &'static str {
    let hash = seed
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    PLACEHOLDERS[(hash as usize) % PLACEHOLDERS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_verbs_map_known_tools_and_fall_back() {
        assert_eq!(tool_verb("read_file"), "reading");
        assert_eq!(tool_verb("run_command"), "running");
        assert_eq!(tool_verb("totally_unknown"), "totally_unknown");
    }

    #[test]
    fn thinking_verb_rotates_with_the_tick() {
        let first = thinking_verb(0);
        let later = thinking_verb(VERB_TICKS);
        assert_ne!(first, later);
        // Stable within a window.
        assert_eq!(thinking_verb(0), thinking_verb(VERB_TICKS - 1));
    }

    #[test]
    fn charm_appears_only_after_a_while() {
        assert!(charm(0, 0).is_none());
        assert!(charm(0, 30).is_some());
    }

    #[test]
    fn placeholder_is_stable_per_seed_and_in_range() {
        let a = placeholder("chat-123");
        assert_eq!(a, placeholder("chat-123"));
        assert!(PLACEHOLDERS.contains(&a));
    }
}

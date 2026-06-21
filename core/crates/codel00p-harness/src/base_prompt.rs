//! The default base operating prompt: rigorous, tool-agnostic instructions for
//! *how* the agent works, injected as a system block each turn.
//!
//! This composes with — and never duplicates — the self block (`self_context`):
//! the self block says *who I am* (identity, capabilities, live run-state); this
//! block says *how I work* (understand first, plan, change carefully, verify
//! before declaring done). It is injected right after the self block and before
//! the project instructions, so a project's `CODEL00P.md` can augment or override
//! this guidance in spirit.
//!
//! Two facets are toggleable from `[agent.behavior]`:
//! - `base_prompt` (default on) — inject this block at all.
//! - `auto_plan` (default on) — include the "lay out a plan" guidance. With it
//!   off, a minimal/manual profile stays quieter and the planning line is
//!   omitted.
//!
//! Kept deliberately tight (this costs tokens every turn) and tool-agnostic (it
//! never names a tool that a tool-set might have disabled — it speaks of "a
//! planning tool" and "the project's tests/build" instead).

/// The fixed core of the base operating prompt — the rigor guidance that is
/// always present when the base prompt is injected.
const CORE: &str = "\
How you work:
- Understand before you act. Read the relevant code and grasp the task and its \
context before editing; do not guess at structure you can inspect.
- Make focused, correct changes that match the surrounding code's style and \
conventions. Prefer the smallest change that fully solves the problem.
- Verify before you declare done. Where applicable, run the project's tests and \
build (or the relevant checks) and confirm they pass. Never claim something \
works, is fixed, or is complete unless you have actually checked it. If you \
cannot verify, say so plainly and state exactly what is unverified.
- Be honest about uncertainty. Don't overstate confidence. Prefer doing the \
work yourself over telling the user to do it.";

/// The optional planning line, appended after the core when `auto_plan` is on.
const PLANNING: &str = "\
- For non-trivial or multi-step work, lay out a short plan up front using the \
planning tool and keep it updated as you make progress.";

/// Render the base operating prompt. `auto_plan` controls whether the planning
/// guidance is included. The returned block is a single system message.
pub fn base_prompt(auto_plan: bool) -> String {
    if auto_plan {
        format!("{CORE}\n{PLANNING}")
    } else {
        CORE.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_core_rigor_guidance() {
        let prompt = base_prompt(true);
        assert!(prompt.contains("Understand before you act"));
        assert!(prompt.contains("Verify before you declare done"));
        assert!(prompt.contains("Never claim"));
        assert!(prompt.contains("cannot verify"));
        assert!(prompt.contains("honest about uncertainty"));
    }

    #[test]
    fn planning_guidance_present_only_when_auto_plan_on() {
        assert!(base_prompt(true).contains("lay out a short plan"));
        assert!(!base_prompt(false).contains("lay out a short plan"));
        // The core rigor guidance is present regardless of the planning toggle.
        assert!(base_prompt(false).contains("Verify before you declare done"));
    }
}

//! Best-effort identity for memory mutations.
//!
//! Memory review records *who* approved/rejected/edited a record. On the
//! scriptable path an explicit `--actor` is still honored; for interactive use we
//! infer it so the human never has to type it. Every step is read-only and
//! infallible — this runs on the default path of every mutation, so it must never
//! panic or block.

use std::process::Command;

/// Infers the acting user: the stored cloud login email, else `git config
/// user.name`, else `$USER`/`$USERNAME`, else `"unknown"`.
pub fn infer_actor() -> String {
    resolve(
        crate::credentials::load().email,
        git_user_name(),
        std::env::var("USER")
            .ok()
            .or_else(|| std::env::var("USERNAME").ok()),
    )
}

/// The pure precedence, separated from its (impure) sources so it can be tested.
fn resolve(email: Option<String>, git: Option<String>, user: Option<String>) -> String {
    non_empty(email)
        .or_else(|| non_empty(git))
        .or_else(|| non_empty(user))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `git config user.name`, or `None` if git is absent, errors, or is unset.
fn git_user_name() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty(String::from_utf8(output.stdout).ok())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolve_prefers_email_then_git_then_user_then_unknown() {
        assert_eq!(
            resolve(
                Some("me@team.dev".into()),
                Some("Git Name".into()),
                Some("user".into())
            ),
            "me@team.dev"
        );
        assert_eq!(
            resolve(None, Some("Git Name".into()), Some("user".into())),
            "Git Name"
        );
        assert_eq!(resolve(None, None, Some("user".into())), "user");
        assert_eq!(resolve(None, None, None), "unknown");
    }

    #[test]
    fn resolve_skips_blank_values() {
        assert_eq!(
            resolve(Some("   ".into()), Some("".into()), Some("ada".into())),
            "ada"
        );
        assert_eq!(resolve(Some("  ".into()), None, None), "unknown");
    }
}

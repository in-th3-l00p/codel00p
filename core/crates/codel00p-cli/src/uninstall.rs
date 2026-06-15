//! Uninstall: remove the installed codel00p binary and, with `--purge`, the
//! `~/.codel00p` data directory.
//!
//! - `codel00p uninstall` — confirm, then remove the running binary; keep data.
//! - `codel00p uninstall --purge` — also remove `~/.codel00p` (config, credentials,
//!   saved sessions, and memory).
//! - `codel00p uninstall --yes` / `-y` — skip the confirmation prompt.
//!
//! The removal is intentionally conservative: data is preserved unless `--purge`
//! is given, and a non-interactive shell must pass `--yes` rather than have us
//! guess. Reinstalling is the curl/irm one-liner, so this is fully reversible
//! short of a `--purge`.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::config::CliResult;
use crate::settings;

/// Parsed `uninstall` options.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    /// Skip the interactive confirmation.
    assume_yes: bool,
    /// Also delete the `~/.codel00p` data directory.
    purge: bool,
}

fn parse(args: &[String]) -> CliResult<Options> {
    let mut options = Options::default();
    for arg in args {
        match arg.as_str() {
            "--yes" | "-y" => options.assume_yes = true,
            "--purge" => options.purge = true,
            other => {
                return Err(format!(
                    "unknown uninstall flag: {other}\nUsage: codel00p uninstall [--purge] [--yes]"
                ));
            }
        }
    }
    Ok(options)
}

pub fn run(args: &[String]) -> CliResult<String> {
    let options = parse(args)?;

    let binary = std::env::current_exe()
        .map_err(|error| format!("cannot locate the codel00p binary to remove: {error}"))?;
    let data_dir = settings::home_dir();
    let data_exists = data_dir.exists();
    let purge = options.purge && data_exists;

    // Show exactly what will be removed before asking — the user confirms with
    // full knowledge of the paths involved.
    eprint!("{}", plan_summary(&binary, &data_dir, data_exists, purge));

    if !options.assume_yes {
        if !io::stdin().is_terminal() {
            return Err(
                "uninstall needs confirmation; re-run with --yes to remove codel00p \
                 non-interactively"
                    .to_string(),
            );
        }
        if !confirm("Remove codel00p?")? {
            return Ok("Uninstall cancelled. Nothing was removed.\n".to_string());
        }
    }

    let mut lines = Vec::new();
    match remove_binary(&binary)? {
        BinaryOutcome::Removed => lines.push(format!("Removed {}", binary.display())),
        BinaryOutcome::Manual => lines.push(format!(
            "Could not delete the running binary on this platform. \
             Remove it manually: {}",
            binary.display()
        )),
    }

    if purge {
        fs::remove_dir_all(&data_dir)
            .map_err(|error| format!("failed to remove {}: {error}", data_dir.display()))?;
        lines.push(format!("Removed {}", data_dir.display()));
    }

    Ok(format!(
        "{}\n",
        closing_message(&lines, &data_dir, data_exists, purge)
    ))
}

/// The human summary of what `uninstall` is about to remove, given the resolved
/// paths and whether data will be purged. Pure so it is unit-testable.
fn plan_summary(binary: &Path, data_dir: &Path, data_exists: bool, purge: bool) -> String {
    let mut summary = String::from("This will uninstall codel00p:\n\n");
    summary.push_str(&format!("  • binary   {}\n", binary.display()));
    if purge {
        summary.push_str(&format!(
            "  • data     {}  (config, credentials, saved sessions, and memory)\n\n",
            data_dir.display()
        ));
        summary.push_str("Removing the data directory is permanent.\n\n");
    } else if data_exists {
        summary.push_str(&format!(
            "\nYour codel00p data is kept at {}\n(config, credentials, saved sessions, and \
             memory). Re-run with --purge to remove it too.\n\n",
            data_dir.display()
        ));
    } else {
        summary.push('\n');
    }
    summary
}

/// The closing message after removal: what was removed plus follow-up guidance
/// (how to delete kept data, the PATH line to tidy, and how to reinstall). Pure.
fn closing_message(removed: &[String], data_dir: &Path, data_exists: bool, purge: bool) -> String {
    let mut message = removed.join("\n");
    message.push('\n');

    if data_exists && !purge {
        message.push_str(&format!(
            "\nYour configuration and memory remain at {}.\nDelete it later with: rm -rf {}\n",
            data_dir.display(),
            data_dir.display()
        ));
    }

    message.push_str(
        "\nIf you added codel00p's install directory to your shell PATH, you can remove\n\
         that line now. Reinstall any time:\n  \
         curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh\n\
         \nThanks for trying codel00p.",
    );
    message
}

/// Outcome of attempting to delete the binary.
enum BinaryOutcome {
    Removed,
    /// The platform locks the running executable (Windows); the user must remove
    /// it by hand. Only constructed on Windows, hence the allow on non-Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    Manual,
}

#[cfg(not(windows))]
fn remove_binary(binary: &Path) -> CliResult<BinaryOutcome> {
    // On Unix a running process keeps its open inode, so deleting the file the
    // binary was launched from is safe and takes effect immediately.
    fs::remove_file(binary)
        .map_err(|error| format!("failed to remove {} ({error})", binary.display()))?;
    Ok(BinaryOutcome::Removed)
}

#[cfg(windows)]
fn remove_binary(_binary: &Path) -> CliResult<BinaryOutcome> {
    // Windows refuses to delete a running executable; report it for manual cleanup.
    Ok(BinaryOutcome::Manual)
}

fn confirm(question: &str) -> CliResult<bool> {
    let mut stderr = io::stderr();
    write!(stderr, "{question} [y/N] ").map_err(|error| error.to_string())?;
    stderr.flush().ok();
    let mut answer = String::new();
    if io::stdin()
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?
        == 0
    {
        return Ok(false);
    }
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn paths() -> (PathBuf, PathBuf) {
        (
            PathBuf::from("/home/dev/.local/bin/codel00p"),
            PathBuf::from("/home/dev/.codel00p"),
        )
    }

    #[test]
    fn parse_accepts_known_flags_in_any_order() {
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert_eq!(
            parse(&["--yes".into()]).unwrap(),
            Options {
                assume_yes: true,
                purge: false
            }
        );
        assert_eq!(
            parse(&["--purge".into(), "-y".into()]).unwrap(),
            Options {
                assume_yes: true,
                purge: true
            }
        );
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let error = parse(&["--force".into()]).unwrap_err();
        assert!(error.contains("unknown uninstall flag"));
        assert!(error.contains("Usage"));
    }

    #[test]
    fn summary_keeps_data_by_default() {
        let (binary, data) = paths();
        let summary = plan_summary(&binary, &data, true, false);
        assert!(summary.contains("codel00p"));
        assert!(summary.contains("binary"));
        assert!(summary.contains("kept at"));
        assert!(summary.contains("--purge"));
        // Without purge, it never claims a permanent data deletion.
        assert!(!summary.contains("permanent"));
    }

    #[test]
    fn summary_warns_when_purging() {
        let (binary, data) = paths();
        let summary = plan_summary(&binary, &data, true, true);
        assert!(summary.contains("data"));
        assert!(summary.contains("permanent"));
        assert!(!summary.contains("Re-run with --purge"));
    }

    #[test]
    fn summary_omits_data_line_when_no_data_dir() {
        let (binary, data) = paths();
        let summary = plan_summary(&binary, &data, false, false);
        assert!(summary.contains("binary"));
        assert!(!summary.contains("kept at"));
    }

    #[test]
    fn closing_message_mentions_kept_data_and_reinstall() {
        let (_, data) = paths();
        let removed = vec!["Removed /home/dev/.local/bin/codel00p".to_string()];
        let message = closing_message(&removed, &data, true, false);
        assert!(message.contains("remain at"));
        assert!(message.contains("rm -rf /home/dev/.codel00p"));
        assert!(message.contains("install.sh"));
    }

    #[test]
    fn closing_message_after_purge_has_no_kept_data_note() {
        let (_, data) = paths();
        let removed = vec![
            "Removed /home/dev/.local/bin/codel00p".to_string(),
            "Removed /home/dev/.codel00p".to_string(),
        ];
        let message = closing_message(&removed, &data, true, true);
        assert!(!message.contains("remain at"));
        assert!(message.contains("Reinstall"));
    }
}

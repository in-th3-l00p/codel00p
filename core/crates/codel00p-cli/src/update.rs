//! Self-update: check GitHub Releases for a newer codel00p, prompt, and replace the
//! running binary in place.
//!
//! - `codel00p update` — check, prompt, and install the latest release.
//! - `codel00p update --check` — report whether an update is available, no install.
//! - `codel00p update --yes` — install without prompting.
//! - `codel00p update --version vX.Y.Z` — install a specific tag.
//!
//! A throttled background check (see [`spawn_background_check`]) refreshes a small
//! cache so the next launch can nudge the user without any startup network call.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::CliResult;

const REPO: &str = "in-th3-l00p/codel00p";
const LATEST_RELEASE_URL: &str = "https://github.com/in-th3-l00p/codel00p/releases/latest";
/// Re-check at most once per day.
const CHECK_INTERVAL_SECS: u64 = 60 * 60 * 24;
const NETWORK_TIMEOUT_SECS: u64 = 12;

/// The version this binary was built as (release tag for published builds, the
/// crate version for local builds). See `build.rs`.
pub(crate) fn current_version() -> &'static str {
    env!("CODEL00P_VERSION")
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

pub fn run(args: &[String]) -> CliResult<String> {
    let mut check_only = false;
    let mut assume_yes = false;
    let mut force = false;
    let mut target_tag: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => check_only = true,
            "--yes" | "-y" => assume_yes = true,
            "--force" => force = true,
            "--version" => {
                index += 1;
                target_tag = Some(
                    args.get(index)
                        .ok_or_else(|| "--version needs a tag, e.g. v0.2.0".to_string())?
                        .clone(),
                );
            }
            other => return Err(format!("unknown update flag: {other}")),
        }
        index += 1;
    }

    let current = current_version();
    let tag = match target_tag {
        Some(tag) => tag,
        None => {
            let release = fetch_latest_release()?;
            release.tag_name
        }
    };
    let latest = tag.trim_start_matches('v');

    // Record what we just learned so background nudges stay fresh.
    write_cache(&UpdateCache {
        last_check: now_secs(),
        latest_version: Some(latest.to_string()),
    });

    let newer = is_newer(latest, current);
    if !newer && !force {
        return Ok(format!(
            "codel00p is up to date (v{current}, latest v{latest}).\n"
        ));
    }

    if check_only {
        return Ok(format!(
            "A new codel00p is available: v{current} → v{latest}.\nRun `codel00p update` to install it.\n"
        ));
    }

    if !assume_yes && !confirm(&format!("Update codel00p v{current} → v{latest}?"))? {
        return Ok("Update cancelled.\n".to_string());
    }

    let target = target_triple()
        .ok_or_else(|| "this platform has no prebuilt release; build from source".to_string())?;
    apply_update(&tag, &target)?;

    Ok(format!(
        "Updated codel00p to v{latest}. Restart any running sessions to use it.\n"
    ))
}

// ---------------------------------------------------------------------------
// Startup nudge (cheap, cache-backed) + background refresh
// ---------------------------------------------------------------------------

/// A one-line "update available" message to print at startup, based solely on the
/// cached result of a previous check — never makes a network call. Returns `None`
/// when up to date, when checks are disabled, or when not attached to a terminal.
pub(crate) fn startup_notice() -> Option<String> {
    if !checks_enabled() || !io::stderr().is_terminal() {
        return None;
    }
    let latest = cached_newer_version()?;
    Some(format!(
        "A new codel00p is available: v{} → v{latest}. Run `codel00p update`.",
        current_version()
    ))
}

/// The cached latest version if it is newer than the running binary, else `None`.
/// Used by both the startup nudge and the TUI header chip.
pub(crate) fn cached_newer_version() -> Option<String> {
    let cache = read_cache()?;
    let latest = cache.latest_version?;
    is_newer(&latest, current_version()).then_some(latest)
}

/// If the cached check is stale (and checks are enabled), refresh it on a detached
/// thread so it never blocks or fails this invocation. The result is only used by a
/// later run; long-lived commands (chat/TUI) reliably complete the refresh.
pub(crate) fn spawn_background_check() {
    if !checks_enabled() {
        return;
    }
    let due = match read_cache() {
        Some(cache) => now_secs().saturating_sub(cache.last_check) >= CHECK_INTERVAL_SECS,
        None => true,
    };
    if !due {
        return;
    }
    std::thread::spawn(|| {
        if let Ok(release) = fetch_latest_release() {
            write_cache(&UpdateCache {
                last_check: now_secs(),
                latest_version: Some(release.tag_name.trim_start_matches('v').to_string()),
            });
        }
    });
}

/// Whether update checks are allowed by the global env kill switch. Set
/// `CODEL00P_DISABLE_UPDATE_CHECK` to any non-empty value to disable all checks,
/// regardless of the per-user `tui.check_updates` setting.
pub(crate) fn checks_enabled() -> bool {
    std::env::var("CODEL00P_DISABLE_UPDATE_CHECK")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_none()
}

/// Performs a live (network) update check and returns the latest version string
/// when it is strictly newer than the running binary, else `None`. This is the
/// blocking core used by the TUI's background startup check; the caller is
/// responsible for running it off the UI task (e.g. via `spawn_blocking`). The
/// cache is refreshed as a side effect so a later run can nudge without a network
/// call. Returns `None` on any network/parse error so a failed check never nags.
pub(crate) fn fetch_newer_version() -> Option<String> {
    let release = fetch_latest_release().ok()?;
    let latest = release.tag_name.trim_start_matches('v').to_string();
    write_cache(&UpdateCache {
        last_check: now_secs(),
        latest_version: Some(latest.clone()),
    });
    newer_or_none(&latest, current_version())
}

/// The version-newer decision a check makes: returns `Some(latest)` only when it
/// is strictly newer than `current`. Factored out of [`fetch_newer_version`] so
/// the decision is unit-testable without any network call.
fn newer_or_none(latest: &str, current: &str) -> Option<String> {
    is_newer(latest, current).then(|| latest.trim_start_matches('v').to_string())
}

// ---------------------------------------------------------------------------
// GitHub release fetch
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
}

fn fetch_latest_release() -> Result<Release, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(NETWORK_TIMEOUT_SECS))
        .build()
        .map_err(|error| format!("failed to build http client: {error}"))?;
    let response = client
        .get(LATEST_RELEASE_URL)
        .header(reqwest::header::USER_AGENT, user_agent())
        .send()
        .map_err(|error| format!("could not reach GitHub: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned {} checking for updates",
            response.status()
        ));
    }
    let tag_name = release_tag_from_url(response.url().as_str())
        .ok_or_else(|| "could not determine latest GitHub release tag".to_string())?;
    Ok(Release { tag_name })
}

fn user_agent() -> String {
    format!("codel00p/{}", current_version())
}

fn release_tag_from_url(url: &str) -> Option<String> {
    let marker = "/releases/tag/";
    let tag = url.split(marker).nth(1)?.split(['?', '#']).next()?.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

// ---------------------------------------------------------------------------
// Download + install
// ---------------------------------------------------------------------------

fn apply_update(tag: &str, target: &str) -> CliResult<()> {
    if cfg!(windows) {
        return Err(
            "automatic update isn't supported on Windows yet — re-run install.ps1".to_string(),
        );
    }

    let asset = format!("codel00p-{target}.tar.gz");
    let base = format!("https://github.com/{REPO}/releases/download/{tag}/{asset}");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(NETWORK_TIMEOUT_SECS * 5))
        .build()
        .map_err(|error| format!("failed to build http client: {error}"))?;

    let archive = download_bytes(&client, &base)?;
    let checksum = download_text(&client, &format!("{base}.sha256")).ok();
    if let Some(checksum) = checksum {
        verify_sha256(&archive, &checksum)?;
    }

    let temp = std::env::temp_dir().join(format!("codel00p-update-{}", now_secs()));
    fs::create_dir_all(&temp).map_err(|error| error.to_string())?;
    let archive_path = temp.join(&asset);
    fs::write(&archive_path, &archive).map_err(|error| error.to_string())?;

    // Reuse the system `tar` (as install.sh does) instead of pulling in a gzip/tar
    // dependency just for this path.
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&temp)
        .status()
        .map_err(|error| format!("failed to run tar: {error}"))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&temp);
        return Err("failed to extract the downloaded archive".to_string());
    }

    let extracted = temp.join("codel00p");
    if !extracted.exists() {
        let _ = fs::remove_dir_all(&temp);
        return Err("downloaded archive did not contain a codel00p binary".to_string());
    }

    let result = replace_running_binary(&extracted);
    let _ = fs::remove_dir_all(&temp);
    result
}

/// Atomically swaps the new binary over the current executable. The new file is
/// staged in the same directory so the final `rename` stays on one filesystem;
/// replacing a running binary is safe on Unix (the live process keeps its inode).
fn replace_running_binary(new_binary: &std::path::Path) -> CliResult<()> {
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let dir = current
        .parent()
        .ok_or_else(|| "cannot locate the install directory".to_string())?;
    let staged = dir.join(format!(".codel00p-update-{}", now_secs()));

    fs::copy(new_binary, &staged).map_err(|error| {
        format!(
            "cannot write to {} ({error}); you may need to re-run the installer",
            dir.display()
        )
    })?;
    set_executable(&staged);

    fs::rename(&staged, &current).map_err(|error| {
        let _ = fs::remove_file(&staged);
        format!("failed to replace {}: {error}", current.display())
    })?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) {}

fn download_bytes(client: &reqwest::blocking::Client, url: &str) -> CliResult<Vec<u8>> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent())
        .send()
        .map_err(|error| format!("download failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("download failed ({}): {url}", response.status()));
    }
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("download failed: {error}"))
}

fn download_text(client: &reqwest::blocking::Client, url: &str) -> CliResult<String> {
    let bytes = download_bytes(client, url)?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

fn verify_sha256(bytes: &[u8], checksum_file: &str) -> CliResult<()> {
    // The `.sha256` sidecar is "<hex>  <filename>"; take the first token.
    let expected = checksum_file
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    if expected.is_empty() {
        return Ok(());
    }
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Version comparison and platform target
// ---------------------------------------------------------------------------

/// Whether `latest` is strictly newer than `current`. Both are compared as
/// `major.minor.patch`, ignoring any leading `v` and pre-release/build suffix. If
/// either cannot be parsed, returns `false` so we never nag on garbage input.
fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let trimmed = value.trim().trim_start_matches('v');
    let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed).trim();
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// The release target triple for this host, matching the asset names produced by
/// `.github/workflows/release.yml`. `None` on unsupported platforms.
fn target_triple() -> Option<String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return None;
    };
    let triple = if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else if cfg!(target_os = "linux") {
        format!("{arch}-unknown-linux-gnu")
    } else if cfg!(target_os = "windows") {
        format!("{arch}-pc-windows-msvc")
    } else {
        return None;
    };
    Some(triple)
}

// ---------------------------------------------------------------------------
// Cache + small helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateCache {
    last_check: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest_version: Option<String>,
}

fn cache_path() -> PathBuf {
    crate::settings::home_dir().join("update-check.json")
}

fn read_cache() -> Option<UpdateCache> {
    let contents = fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(cache: &UpdateCache) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(path, contents);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
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

    #[test]
    fn parses_versions_with_prefix_and_suffix() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v0.2"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.4.0-rc.1"), Some((1, 4, 0)));
        assert_eq!(parse_version("2.0.0+build5"), Some((2, 0, 0)));
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn newer_comparison() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        // Unparseable inputs never trigger a nag.
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("0.2.0", "garbage"));
    }

    #[test]
    fn newer_or_none_returns_only_strictly_newer() {
        assert_eq!(newer_or_none("0.9.0", "0.8.0").as_deref(), Some("0.9.0"));
        assert_eq!(newer_or_none("v1.0.0", "0.9.9").as_deref(), Some("1.0.0"));
        assert_eq!(newer_or_none("0.8.0", "0.8.0"), None);
        assert_eq!(newer_or_none("0.7.0", "0.8.0"), None);
        assert_eq!(newer_or_none("garbage", "0.8.0"), None);
    }

    #[test]
    fn parses_latest_release_redirect_url() {
        assert_eq!(
            release_tag_from_url("https://github.com/in-th3-l00p/codel00p/releases/tag/v0.3.0")
                .as_deref(),
            Some("v0.3.0")
        );
        assert_eq!(
            release_tag_from_url(
                "https://github.com/in-th3-l00p/codel00p/releases/tag/v0.3.0?expanded=true"
            )
            .as_deref(),
            Some("v0.3.0")
        );
        assert!(release_tag_from_url("https://github.com/in-th3-l00p/codel00p/releases").is_none());
    }

    #[test]
    fn sha256_matches_known_vector() {
        // SHA-256 of "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_accepts_matching_sidecar() {
        let bytes = b"codel00p";
        let sidecar = format!("{}  codel00p-x86_64-apple-darwin.tar.gz", sha256_hex(bytes));
        assert!(verify_sha256(bytes, &sidecar).is_ok());
        assert!(verify_sha256(bytes, "deadbeef  archive").is_err());
    }

    #[test]
    fn target_triple_is_known_on_supported_hosts() {
        // The host running tests is one of our release targets.
        let triple = target_triple().expect("supported host");
        assert!(
            triple.contains("apple-darwin")
                || triple.contains("linux")
                || triple.contains("windows")
        );
    }

    #[test]
    fn cache_roundtrips() {
        let cache = UpdateCache {
            last_check: 42,
            latest_version: Some("0.3.0".to_string()),
        };
        let json = serde_json::to_string(&cache).unwrap();
        let back: UpdateCache = serde_json::from_str(&json).unwrap();
        assert_eq!(back.last_check, 42);
        assert_eq!(back.latest_version.as_deref(), Some("0.3.0"));
    }
}

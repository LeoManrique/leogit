//! In-app update check against the GitHub Releases feed `deploy_releases.sh`
//! publishes to — a port of `LeoSync`'s updater.
//!
//! One unauthenticated `releases/latest` request, a plain three-part numeric
//! version compare against this build's `CARGO_PKG_VERSION`, and no
//! auto-install: on macOS/Linux the frontend offers the `install.sh` one-liner
//! to run in a terminal, on Windows a link to the release page's installer.
//! Failures (offline, rate-limited, GitHub down) surface as `Err` so the
//! frontend can retry quietly later — never as user-facing errors.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The repo whose releases `deploy_releases.sh` creates; tags are `v<x.y.z>`.
const RELEASES_URL: &str = "https://api.github.com/repos/LeoManrique/leogit/releases/latest";

/// GitHub answers in well under a second; this only catches a stalled link.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The upgrade one-liner for macOS/Linux — the same installer the `README`
/// documents. It replaces the app in place; the user relaunches when ready.
const INSTALL_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash";

/// A newer release than this build. Kept `snake_case` on the wire to match
/// the rest of our Tauri payloads.
#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    /// Latest released version, `v` prefix stripped.
    pub version: String,
    /// The GitHub release page (download assets + notes).
    pub url: String,
    /// Terminal one-liner that upgrades in place; `None` on Windows, where
    /// the release-page download is the path instead.
    pub install_command: Option<String>,
}

/// The two fields we use from the GitHub release object. `#[serde(default)]`
/// so an unexpected payload shape degrades to empty strings (and therefore
/// "no update") instead of a decode error.
#[derive(Deserialize)]
struct GithubRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
}

/// Debug-only override so the update chip can be exercised without publishing
/// a release: `LEOGIT_FAKE_UPDATE=9.9.9 pnpm tauri dev` makes the check report
/// that version, pointing at the real releases page. Compiled out entirely in
/// release builds.
#[cfg(debug_assertions)]
fn fake_update() -> Option<UpdateInfo> {
    let version = std::env::var("LEOGIT_FAKE_UPDATE").ok().filter(|v| !v.is_empty())?;
    eprintln!("[update] LEOGIT_FAKE_UPDATE set — reporting v{version}");
    Some(UpdateInfo {
        version,
        url: "https://github.com/LeoManrique/leogit/releases/latest".to_string(),
        install_command: install_command(),
    })
}

/// Ask GitHub Releases whether a version newer than this build exists.
/// `Ok(None)` means this build is current (or the latest tag is malformed,
/// which can only ever compare low — see [`parse3`]).
///
/// # Errors
/// Returns `Err` when the request can't be built, fails, times out, or the
/// response is a non-success status or undecodable body. Callers treat this
/// as "couldn't check", not "no update".
#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    #[cfg(debug_assertions)]
    if let Some(info) = fake_update() {
        return Ok(Some(info));
    }

    let current = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        // GitHub's API rejects requests without a User-Agent.
        .user_agent(concat!("leogit/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("update check failed: {e}"))?;
    let release: GithubRelease = client
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update check failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("update check failed: {e}"))?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer(&latest, current) {
        return Ok(None);
    }
    eprintln!("[update] v{latest} available (running v{current})");
    Ok(Some(UpdateInfo {
        version: latest,
        url: release.html_url,
        install_command: install_command(),
    }))
}

/// The per-platform upgrade path: a shell one-liner where `install.sh` runs,
/// nothing on Windows (the frontend offers the release-page download there).
fn install_command() -> Option<String> {
    if cfg!(target_os = "windows") {
        None
    } else {
        Some(INSTALL_COMMAND.to_string())
    }
}

/// Strictly-newer test over three-part numeric versions — the same ordering
/// `sort -V` gives our plain `x.y.z` tags in `deploy_releases.sh`. Equal
/// versions are not newer, so a just-updated build goes quiet.
fn is_newer(latest: &str, current: &str) -> bool {
    parse3(latest) > parse3(current)
}

/// Split on `.` and parse up to three numeric parts; non-numeric or missing
/// parts become 0, so a malformed tag can only ever compare low — it can
/// never panic or spuriously announce an update.
fn parse3(v: &str) -> (u64, u64, u64) {
    let mut parts = v.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_is_numeric_not_lexicographic() {
        assert!(is_newer("0.1.27", "0.1.26"));
        assert!(is_newer("0.2.0", "0.1.26"));
        assert!(is_newer("1.0.0", "0.9.9"));
        // Lexicographic compare would get this one backwards.
        assert!(is_newer("0.1.30", "0.1.4"));
        assert!(!is_newer("0.1.26", "0.1.26"), "equal is not newer");
        assert!(!is_newer("0.1.25", "0.1.26"));
    }

    #[test]
    fn malformed_versions_compare_low_and_never_panic() {
        assert_eq!(parse3("0.7"), (0, 7, 0));
        assert_eq!(parse3("0.7.0-beta"), (0, 7, 0));
        assert_eq!(parse3("garbage"), (0, 0, 0));
        assert_eq!(parse3(""), (0, 0, 0));
        assert!(!is_newer("not-a-version", "0.1.0"));
    }

    #[test]
    fn install_command_matches_platform() {
        let cmd = install_command();
        if cfg!(target_os = "windows") {
            assert!(cmd.is_none(), "Windows updates via the release page");
        } else {
            assert!(
                cmd.is_some_and(|c| c.starts_with("curl -fsSL https://")),
                "macOS/Linux update via the install.sh one-liner"
            );
        }
    }
}

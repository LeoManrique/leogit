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

/// The fields we use from the GitHub release object. `#[serde(default)]`
/// so an unexpected payload shape degrades to empty values (and therefore
/// "no update") instead of a decode error.
#[derive(Deserialize)]
struct GithubRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    #[serde(default)]
    name: String,
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
    // `deploy_releases.sh` runs once per platform onto one shared release, so
    // the first platform to finish publishes a release the others aren't in
    // yet. Announcing that would send a Windows user to a page with only a
    // macOS zip — and worse on macOS/Linux, where `install.sh` kills the
    // running app before it discovers the artifact is missing. So only offer
    // an update we can actually complete.
    let wanted = artifact_name(&latest);
    if !release.assets.iter().any(|a| a.name == wanted) {
        eprintln!("[update] v{latest} exists but has no {wanted} yet — staying quiet");
        return Ok(None);
    }
    eprintln!("[update] v{latest} available (running v{current})");
    Ok(Some(UpdateInfo {
        version: latest,
        url: release.html_url,
        install_command: install_command(),
    }))
}

/// The release asset this host needs, in the exact shape `deploy_releases.sh`
/// uploads and `install.sh` downloads: `LeoGit-<ver>-<platform>-<arch>.<ext>`
/// (Windows swaps the extension for a `-setup.exe` suffix). Kept byte-identical
/// to those scripts — a mismatch here would either hide real updates or offer
/// one `install.sh` then fails to find.
fn artifact_name(version: &str) -> String {
    // `uname -m` values, normalised the way `install.sh` normalises them.
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    if cfg!(target_os = "windows") {
        format!("LeoGit-{version}-windows-{arch}-setup.exe")
    } else if cfg!(target_os = "macos") {
        format!("LeoGit-{version}-macOS-{arch}.zip")
    } else {
        format!("LeoGit-{version}-linux-{arch}.AppImage")
    }
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
/// versions are not newer, so a just-updated build goes quiet. A tag we can't
/// parse is never newer: we'd have no artifact name to offer for it anyway.
fn is_newer(latest: &str, current: &str) -> bool {
    match (parse3(latest), parse3(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Parse exactly three numeric parts. Anything else — a prerelease suffix
/// (`0.2.0-beta.1`), build metadata (`0.1.28+build.5`), a two- or four-part
/// tag — returns `None` rather than being coerced.
///
/// Coercing is what a lenient parse gets wrong in *both* directions:
/// `0.2.0-beta.1` would collapse to `(0, 2, 0)` and announce a phantom update
/// over `0.1.27`, while `0.1.28+build.5` would collapse to `(0, 1, 0)` and
/// hide a real one. `deploy_releases.sh` only regex-validates the version when
/// one is passed as an argument, so a tag like that isn't purely theoretical.
fn parse3(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    // A fourth part means this isn't the `x.y.z` shape we publish.
    if parts.next().is_some() {
        return None;
    }
    Some(parsed)
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
    fn only_plain_three_part_versions_parse() {
        assert_eq!(parse3("0.1.26"), Some((0, 1, 26)));
        // Everything below would be silently coerced by a lenient parse — and
        // each coercion is a wrong answer, not just an imprecise one.
        assert_eq!(parse3("0.2.0-beta.1"), None, "prerelease is not a release");
        assert_eq!(parse3("0.1.28+build.5"), None, "build metadata");
        assert_eq!(parse3("0.7"), None, "two parts");
        assert_eq!(parse3("1.2.3.4"), None, "four parts");
        assert_eq!(parse3("garbage"), None);
        assert_eq!(parse3(""), None);
    }

    #[test]
    fn unparseable_tags_never_announce_an_update() {
        // A lenient parse reads this as (0, 2, 0) and announces a phantom
        // update over 0.1.27 — the tag has no matching release artifact.
        assert!(!is_newer("0.2.0-beta.1", "0.1.27"));
        assert!(!is_newer("1.0.0-rc1", "0.1.27"));
        assert!(!is_newer("not-a-version", "0.1.0"));
        // …and an unparseable *current* version can't be leapfrogged either.
        assert!(!is_newer("0.2.0", "garbage"));
    }

    #[test]
    fn artifact_name_matches_what_the_release_scripts_publish() {
        // Golden strings, byte-identical to what `deploy_releases.sh` uploads
        // and `install.sh` downloads by name. If either script's naming
        // changes this must change with it — a name that drifts silently
        // hides every future update instead of failing loudly.
        let name = artifact_name("0.1.27");
        let published = [
            "LeoGit-0.1.27-macOS-arm64.zip",
            "LeoGit-0.1.27-macOS-amd64.zip",
            "LeoGit-0.1.27-linux-arm64.AppImage",
            "LeoGit-0.1.27-linux-amd64.AppImage",
            "LeoGit-0.1.27-windows-arm64-setup.exe",
            "LeoGit-0.1.27-windows-amd64-setup.exe",
        ];
        assert!(
            published.contains(&name.as_str()),
            "{name} is not a name the release scripts publish"
        );
        // …and it's this host's, not another platform's.
        let platform = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "linux"
        };
        assert!(name.contains(platform), "{name} is not for this host");
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

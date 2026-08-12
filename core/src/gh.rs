use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

/// `gh` API queries (auth status, repo list) are quick metadata calls, so they
/// fail fast when the network is down rather than hanging the Clone dialog.
const GH_QUERY_TIMEOUT: Duration = Duration::from_secs(20);

/// `gh` operations that transfer a repo (publish/clone) get a generous budget —
/// a real push/clone can take a while — while still capping a wedged process.
const GH_TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);

// `check_auth` stays so future gh-backed features (e.g. `gh project create`)
// can gate themselves on the user having `gh` authenticated. The PR-list /
// PR-checks / PR-create commands were removed when the PR view was retired
// from the UI — re-add them if/when the PR overview ships again.
pub fn check_auth() -> bool {
    let mut cmd = Command::new("gh");
    cmd.arg("auth").arg("status");
    super::process::hide_console(&mut cmd);
    // A spawn failure (gh missing) or timeout both mean "can't confirm auth" → false.
    super::process::run_timed(cmd, "gh auth status", GH_QUERY_TIMEOUT)
        .is_ok_and(|out| out.status.success())
}

/// Map a `run_timed` error to a Clone-dialog-friendly message: keep the
/// "timed out" text when the network stalled, otherwise assume `gh` is missing.
fn gh_unavailable(err: &str) -> String {
    if err.contains("timed out") {
        err.to_string()
    } else {
        "GitHub CLI (gh) is not installed.".to_string()
    }
}

/// `gh repo list` emits camelCase JSON; this mirrors only the fields we use.
#[derive(Deserialize)]
struct GhRepoRaw {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "isPrivate")]
    is_private: bool,
    /// Last push (code activity) as an ISO-8601 timestamp — GitHub's "Updated".
    #[serde(rename = "pushedAt", default)]
    pushed_at: String,
}

/// A repository surfaced in the GitHub tab of the Clone dialog. Kept snake_case
/// on the wire to match the rest of our Tauri payloads.
#[derive(Serialize)]
pub struct GhRepo {
    pub name_with_owner: String,
    pub name: String,
    pub description: String,
    pub is_private: bool,
    /// ISO-8601 last-push timestamp; the frontend sorts "recently modified" on it.
    pub pushed_at: String,
}

/// List the signed-in user's GitHub repositories via the `gh` CLI, most
/// recently pushed first. Includes forks (so e.g. a forked repo can still be
/// cloned from the dialog) but skips archived repos. Errors carry a friendly
/// message when `gh` is missing or unauthenticated so the Clone dialog can
/// point the user at a fix.
pub fn gh_repo_list(limit: u32) -> Result<Vec<GhRepo>, String> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "repo",
        "list",
        "--no-archived",
        "--limit",
        &limit.to_string(),
        "--json",
        "nameWithOwner,name,description,isPrivate,pushedAt",
    ]);
    super::process::hide_console(&mut cmd);
    let output = super::process::run_timed(cmd, "gh repo list", GH_QUERY_TIMEOUT)
        .map_err(|e| gh_unavailable(&e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            "gh is not authenticated. Run `gh auth login`.".to_string()
        } else {
            stderr.to_string()
        });
    }
    let raw: Vec<GhRepoRaw> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Could not parse gh output: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| GhRepo {
            name_with_owner: r.name_with_owner,
            name: r.name,
            description: r.description,
            is_private: r.is_private,
            pushed_at: r.pushed_at,
        })
        .collect())
}

/// Publish a local repository to GitHub via `gh repo create`. Creates the
/// remote repo under the authenticated user, wires it up as the `origin` remote
/// of the local repo, and pushes the current branch — the same one-shot flow as
/// GitHub Desktop's "Publish Repository". `gh` supplies the auth, so this works
/// for private repos without any token plumbing on our side.
///
/// `name` is the GitHub repository name (may be `owner/name` to target an org).
/// An empty `description` is omitted rather than sent as a blank value.
///
/// A transfer that can run for minutes, so it delegates to
/// [`process::run_blocking`] rather than pinning a tokio core worker.
///
/// # Errors
/// When `gh` is missing/unauthenticated or `gh repo create` fails.
pub async fn gh_publish_repo(
    repo_path: String,
    name: String,
    description: String,
    is_private: bool,
) -> Result<(), String> {
    super::process::run_blocking(move || {
        let name = name.trim();
        if name.is_empty() {
            return Err("Repository name is required.".to_string());
        }
        let visibility = if is_private { "--private" } else { "--public" };
        let description = description.trim();
        let mut args: Vec<&str> = vec![
            "repo", "create", name, "--source", &repo_path, "--remote", "origin", "--push",
            visibility,
        ];
        if !description.is_empty() {
            args.push("--description");
            args.push(description);
        }

        let mut cmd = Command::new("gh");
        cmd.args(&args);
        super::process::hide_console(&mut cmd);
        let output = super::process::run_timed(cmd, "gh repo create", GH_TRANSFER_TIMEOUT)
            .map_err(|e| gh_unavailable(&e))?;
        if !output.status.success() {
            // gh writes its diagnostics (auth, name collision, etc.) to stderr.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            return Err(if stderr.is_empty() {
                "gh repo create failed. Is `gh` authenticated? Run `gh auth login`.".to_string()
            } else {
                stderr.to_string()
            });
        }
        Ok(())
    })
    .await?
}

/// Clone a GitHub repo by `owner/name` into `target_path` using `gh repo clone`
/// so it inherits the user's `gh` auth (private repos work without a prompt).
/// Returns the absolute path of the cloned repo. Runs on the blocking pool —
/// see [`process::run_blocking`].
///
/// # Errors
/// When `gh` is missing, the destination can't be prepared, or the clone fails.
pub async fn gh_clone(name_with_owner: String, target_path: String) -> Result<String, String> {
    super::process::run_blocking(move || {
        let target = super::git::prepare_clone_target(&target_path)?;
        let mut cmd = Command::new("gh");
        cmd.args(["repo", "clone", &name_with_owner, &target]);
        super::process::hide_console(&mut cmd);
        let output = super::process::run_timed(cmd, "gh repo clone", GH_TRANSFER_TIMEOUT)
            .map_err(|e| gh_unavailable(&e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(target)
    })
    .await?
}

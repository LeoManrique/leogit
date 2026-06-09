use serde::{Deserialize, Serialize};
use std::process::Command;

// `check_auth` stays so future gh-backed features (e.g. `gh project create`)
// can gate themselves on the user having `gh` authenticated. The PR-list /
// PR-checks / PR-create commands were removed when the PR view was retired
// from the UI — re-add them if/when the PR overview ships again.
#[tauri::command]
pub fn check_auth() -> bool {
    let mut cmd = Command::new("gh");
    cmd.arg("auth").arg("status");
    let output = super::process::hide_console(&mut cmd).output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
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
/// recently pushed first. Errors carry a friendly message when `gh` is missing
/// or unauthenticated so the Clone dialog can point the user at a fix.
#[tauri::command]
pub fn gh_repo_list(limit: u32) -> Result<Vec<GhRepo>, String> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "repo",
        "list",
        "--no-archived",
        "--source",
        "--limit",
        &limit.to_string(),
        "--json",
        "nameWithOwner,name,description,isPrivate,pushedAt",
    ]);
    let output = super::process::hide_console(&mut cmd)
        .output()
        .map_err(|_| "GitHub CLI (gh) is not installed.".to_string())?;
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

/// Clone a GitHub repo by `owner/name` into `target_path` using `gh repo clone`
/// so it inherits the user's `gh` auth (private repos work without a prompt).
/// Returns the absolute path of the cloned repo.
#[tauri::command]
pub fn gh_clone(name_with_owner: String, target_path: String) -> Result<String, String> {
    let target = super::git::prepare_clone_target(&target_path)?;
    let mut cmd = Command::new("gh");
    cmd.args(["repo", "clone", &name_with_owner, &target]);
    let output = super::process::hide_console(&mut cmd)
        .output()
        .map_err(|_| "GitHub CLI (gh) is not installed.".to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(target)
}

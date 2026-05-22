use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i32,
    pub title: String,
    pub state: String,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    // Added fields (frontend-visible)
    pub body: String,
    pub is_draft: bool,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub review_decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRCheck {
    pub name: String,
    // Added fields
    pub state: String,
    pub bucket: String,
    pub link: Option<String>,
    pub workflow: Option<String>,
}

// Internal struct for JSON deserialization from gh output
#[derive(Debug, Deserialize)]
struct GhAuthor {
    #[serde(default)]
    login: String,
}

#[derive(Debug, Deserialize)]
struct GhPullRequest {
    number: i32,
    title: String,
    state: String,
    #[serde(default)]
    author: Option<GhAuthor>,
    #[serde(rename = "createdAt", default)]
    created_at: String,
    #[serde(rename = "updatedAt", default)]
    updated_at: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    body: String,
    #[serde(rename = "isDraft", default)]
    is_draft: bool,
    #[serde(rename = "baseRefName", default)]
    base_ref_name: String,
    #[serde(rename = "headRefName", default)]
    head_ref_name: String,
    #[serde(rename = "reviewDecision", default)]
    review_decision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhCheck {
    #[serde(default)]
    name: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    workflow: Option<String>,
}

const PR_JSON_FIELDS: &str =
    "number,title,state,author,createdAt,updatedAt,url,body,isDraft,baseRefName,headRefName,reviewDecision";

fn convert_pr(g: GhPullRequest) -> PullRequest {
    PullRequest {
        number: g.number,
        title: g.title,
        state: g.state,
        author: g.author.map(|a| a.login).unwrap_or_default(),
        created_at: g.created_at,
        updated_at: g.updated_at,
        url: g.url,
        body: g.body,
        is_draft: g.is_draft,
        base_ref_name: g.base_ref_name,
        head_ref_name: g.head_ref_name,
        review_decision: g
            .review_decision
            .filter(|s| !s.is_empty()),
    }
}

#[tauri::command]
pub fn check_auth() -> bool {
    let output = Command::new("gh").arg("auth").arg("status").output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

#[tauri::command]
pub fn list_prs(repo_path: String, state: String) -> Result<Vec<PullRequest>, String> {
    let output = Command::new("gh")
        .arg("pr")
        .arg("list")
        .arg("--json")
        .arg(PR_JSON_FIELDS)
        .arg("--state")
        .arg(&state)
        .arg("--limit")
        .arg("30")
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr list: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr list failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let gh_prs: Vec<GhPullRequest> =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse PR JSON: {}", e))?;

    let prs = gh_prs.into_iter().map(convert_pr).collect();
    Ok(prs)
}

#[tauri::command]
pub fn get_pr_checks(repo_path: String, number: i32) -> Result<Vec<PRCheck>, String> {
    let output = Command::new("gh")
        .arg("pr")
        .arg("checks")
        .arg(number.to_string())
        .arg("--json")
        .arg("name,state,bucket,link,workflow")
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr checks: {}", e))?;

    // gh pr checks exits non-zero when any check is pending/failing, but still
    // emits valid JSON to stdout. Try parsing stdout first; only treat as a
    // hard error if both the exit code is non-zero and JSON parse fails.
    let stdout = String::from_utf8_lossy(&output.stdout);
    match serde_json::from_str::<Vec<GhCheck>>(&stdout) {
        Ok(checks) => {
            let pr_checks = checks
                .into_iter()
                .map(|c| PRCheck {
                    name: c.name,
                    state: c.state,
                    bucket: c.bucket,
                    link: c.link.filter(|s| !s.is_empty()),
                    workflow: c.workflow.filter(|s| !s.is_empty()),
                })
                .collect();
            Ok(pr_checks)
        }
        Err(parse_err) => {
            if output.status.success() {
                // Empty stdout (no checks) — return an empty list
                if stdout.trim().is_empty() {
                    return Ok(Vec::new());
                }
                Err(format!("Failed to parse checks JSON: {}", parse_err))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let msg = if stderr.trim().is_empty() {
                    stdout.to_string()
                } else {
                    stderr.to_string()
                };
                Err(format!("gh pr checks failed: {}", msg.trim()))
            }
        }
    }
}

#[tauri::command]
pub fn create_pr(
    repo_path: String,
    title: String,
    body: String,
    base: String,
    draft: bool,
) -> Result<String, String> {
    let mut cmd = Command::new("gh");
    cmd.arg("pr")
        .arg("create")
        .arg("--title")
        .arg(&title)
        .arg("--body")
        .arg(&body);

    if !base.is_empty() {
        cmd.arg("--base").arg(&base);
    }

    if draft {
        cmd.arg("--draft");
    }

    let output = cmd
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr create: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr create failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout.trim().to_string();

    Ok(url)
}

#[tauri::command]
pub fn create_pr_fill(repo_path: String, base: String, draft: bool) -> Result<String, String> {
    let mut cmd = Command::new("gh");
    cmd.arg("pr").arg("create").arg("--fill");

    if !base.is_empty() {
        cmd.arg("--base").arg(&base);
    }

    if draft {
        cmd.arg("--draft");
    }

    let output = cmd
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr create: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr create --fill failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout.trim().to_string();

    Ok(url)
}

#[tauri::command]
pub fn checkout_pr(repo_path: String, number: i32) -> Result<(), String> {
    let output = Command::new("gh")
        .arg("pr")
        .arg("checkout")
        .arg(number.to_string())
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr checkout: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr checkout failed: {}", stderr));
    }

    Ok(())
}

#[tauri::command]
pub fn get_current_branch_pr(
    repo_path: String,
    branch: String,
) -> Result<Option<PullRequest>, String> {
    if branch.is_empty() {
        return Ok(None);
    }

    let output = Command::new("gh")
        .arg("pr")
        .arg("list")
        .arg("--head")
        .arg(&branch)
        .arg("--state")
        .arg("open")
        .arg("--json")
        .arg(PR_JSON_FIELDS)
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to execute gh pr list: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr list failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let gh_prs: Vec<GhPullRequest> =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse PR JSON: {}", e))?;

    Ok(gh_prs.into_iter().next().map(convert_pr))
}

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
// can gate themselves on the user having `gh` authenticated. Neither client
// currently reads it: every other function's error text already distinguishes
// "gh missing" from "not authenticated".
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

/// gh writes its diagnostics (auth, name collisions, missing GitHub remote)
/// to stderr; surface it verbatim and trimmed, with `fallback` for the rare
/// silent failure.
fn stderr_or(output: &std::process::Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        fallback.to_string()
    } else {
        stderr.to_string()
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
        return Err(stderr_or(
            &output,
            "gh is not authenticated. Run `gh auth login`.",
        ));
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
            return Err(stderr_or(
                &output,
                "gh repo create failed. Is `gh` authenticated? Run `gh auth login`.",
            ));
        }
        Ok(())
    })
    .await?
}

// ---------------------------------------------------------------------------
// Pull requests
// ---------------------------------------------------------------------------
//
// Unlike `gh repo create`, the `gh pr` subcommands have no path flag — they
// resolve their repository from the working directory — so this is the one
// section that sets `current_dir`. The subprocess has no TTY, which makes gh
// disable its interactive prompts by itself: an unpushed branch or a missing
// GitHub remote fails with gh's own message instead of hanging on a question.

/// Fields requested from `gh pr list`; must line up with [`PullRequestRaw`].
const PR_JSON_FIELDS: &str = "number,title,state,author,createdAt,updatedAt,url,body,isDraft,\
                              baseRefName,headRefName,reviewDecision,additions,deletions,\
                              changedFiles";

/// `gh` nests the author as an object; only the login matters here.
#[derive(Deserialize)]
struct PrAuthorRaw {
    #[serde(default)]
    login: String,
}

/// `gh pr list` emits camelCase JSON; this mirrors only the fields we use.
/// Everything except the identity fields is `#[serde(default)]`, so a field
/// gh omits (or nulls, e.g. a deleted "ghost" author) parses instead of
/// failing the whole list.
#[derive(Deserialize)]
struct PullRequestRaw {
    number: u32,
    title: String,
    state: String,
    #[serde(default)]
    author: Option<PrAuthorRaw>,
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
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(rename = "changedFiles", default)]
    changed_files: u32,
}

/// One pull request as the PR view renders it. Kept `snake_case` on the
/// wire to match the rest of our payloads.
#[derive(Serialize)]
pub struct PullRequest {
    pub number: u32,
    pub title: String,
    /// `"OPEN"`, `"CLOSED"`, or `"MERGED"`, as gh reports it.
    pub state: String,
    /// The author's login; empty for a deleted ("ghost") account.
    pub author: String,
    /// ISO-8601 timestamps, straight from gh.
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    pub body: String,
    pub is_draft: bool,
    /// The branch the PR merges into / from (`main` ← `feature`).
    pub base_ref_name: String,
    pub head_ref_name: String,
    /// `"APPROVED"` / `"CHANGES_REQUESTED"` / `"REVIEW_REQUIRED"`; `None`
    /// when the repo requires no review (gh reports that as an empty
    /// string, normalised away here).
    pub review_decision: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
}

/// One CI check row from `gh pr checks`.
#[derive(Serialize)]
pub struct PrCheck {
    pub name: String,
    /// Raw state, e.g. `"SUCCESS"`, `"FAILURE"`, `"IN_PROGRESS"`.
    pub state: String,
    /// gh's rollup bucket — `"pass"`, `"fail"`, `"pending"`, `"skipping"`,
    /// or `"cancel"` — which is what the UI colours by.
    pub bucket: String,
    pub link: Option<String>,
    pub workflow: Option<String>,
}

#[derive(Deserialize)]
struct PrCheckRaw {
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

/// Decode `gh pr list` output into the public shape. Pure, so the mapping —
/// author flattening, empty-review normalisation — is testable without gh.
fn parse_pr_list(stdout: &[u8]) -> Result<Vec<PullRequest>, String> {
    let raw: Vec<PullRequestRaw> =
        serde_json::from_slice(stdout).map_err(|e| format!("Could not parse gh output: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| PullRequest {
            number: r.number,
            title: r.title,
            state: r.state,
            author: r.author.map(|a| a.login).unwrap_or_default(),
            created_at: r.created_at,
            updated_at: r.updated_at,
            url: r.url,
            body: r.body,
            is_draft: r.is_draft,
            base_ref_name: r.base_ref_name,
            head_ref_name: r.head_ref_name,
            review_decision: r.review_decision.filter(|d| !d.is_empty()),
            additions: r.additions,
            deletions: r.deletions,
            changed_files: r.changed_files,
        })
        .collect())
}

/// Decode `gh pr checks --json` stdout; `None` when it isn't valid JSON.
/// Deliberately blind to the exit status — see [`get_pr_checks`].
fn parse_pr_checks(stdout: &[u8]) -> Option<Vec<PrCheck>> {
    let raw: Vec<PrCheckRaw> = serde_json::from_slice(stdout).ok()?;
    Some(
        raw.into_iter()
            .map(|r| PrCheck {
                name: r.name,
                state: r.state,
                bucket: r.bucket,
                link: r.link,
                workflow: r.workflow,
            })
            .collect(),
    )
}

/// The repository's pull requests via `gh pr list`, in gh's own order
/// (newest first) — never re-sorted. `state` is gh's filter: `"open"`,
/// `"closed"`, `"merged"`, or `"all"`. Capped at 30, like the retired
/// TUI's list.
///
/// # Errors
/// When `gh` is missing/unauthenticated, the repo has no GitHub remote, or
/// the output cannot be parsed.
pub fn list_prs(repo_path: &str, state: &str) -> Result<Vec<PullRequest>, String> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "pr",
        "list",
        "--state",
        state,
        "--limit",
        "30",
        "--json",
        PR_JSON_FIELDS,
    ]);
    cmd.current_dir(repo_path);
    super::process::hide_console(&mut cmd);
    let output = super::process::run_timed(cmd, "gh pr list", GH_QUERY_TIMEOUT)
        .map_err(|e| gh_unavailable(&e))?;
    if !output.status.success() {
        return Err(stderr_or(
            &output,
            "gh is not authenticated. Run `gh auth login`.",
        ));
    }
    parse_pr_list(&output.stdout)
}

/// CI status for one PR via `gh pr checks --json`.
///
/// The one quirk worth preserving verbatim from the retired implementation:
/// `gh pr checks` exits **non-zero** whenever any check is pending or
/// failing while still writing valid JSON to stdout. So stdout is parsed
/// first and the exit code alone is never treated as a failure; only when
/// there is no parseable JSON (no checks configured, auth trouble) does
/// stderr become the error.
///
/// # Errors
/// When `gh` is missing, or it produced no check data — its stderr verbatim
/// ("no checks reported…" for a PR without CI).
pub fn get_pr_checks(repo_path: &str, number: u32) -> Result<Vec<PrCheck>, String> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "pr",
        "checks",
        &number.to_string(),
        "--json",
        "name,state,bucket,link,workflow",
    ]);
    cmd.current_dir(repo_path);
    super::process::hide_console(&mut cmd);
    let output = super::process::run_timed(cmd, "gh pr checks", GH_QUERY_TIMEOUT)
        .map_err(|e| gh_unavailable(&e))?;
    if let Some(checks) = parse_pr_checks(&output.stdout) {
        return Ok(checks);
    }
    Err(stderr_or(
        &output,
        "gh pr checks failed. Is `gh` authenticated? Run `gh auth login`.",
    ))
}

/// Open a pull request from the current branch via `gh pr create`,
/// returning the new PR's URL. An empty `base` targets the repository's
/// default branch (the flag is omitted). The branch must already be pushed:
/// without a TTY gh cannot ask "where should we push?", so an unpushed
/// branch fails with gh's own message — fire-and-find-out, like publish.
///
/// # Errors
/// When the trimmed title is empty, `gh` is missing/unauthenticated, or
/// `gh pr create` fails (unpushed branch, a PR already exists, …).
pub fn create_pr(
    repo_path: &str,
    title: &str,
    body: &str,
    base: &str,
    draft: bool,
) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Pull request title is required.".to_string());
    }
    let body = body.trim();
    let base = base.trim();
    let mut args: Vec<&str> = vec!["pr", "create", "--title", title, "--body", body];
    if !base.is_empty() {
        args.push("--base");
        args.push(base);
    }
    if draft {
        args.push("--draft");
    }

    let mut cmd = Command::new("gh");
    cmd.args(&args);
    cmd.current_dir(repo_path);
    super::process::hide_console(&mut cmd);
    let output = super::process::run_timed(cmd, "gh pr create", GH_QUERY_TIMEOUT)
        .map_err(|e| gh_unavailable(&e))?;
    if !output.status.success() {
        return Err(stderr_or(
            &output,
            "gh pr create failed. Is `gh` authenticated? Run `gh auth login`.",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check out a PR's branch via `gh pr checkout` — fetches the head ref
/// (from a fork too) and creates or switches to a local tracking branch. A
/// transfer, so it runs on the blocking pool with the generous budget; a
/// dirty working tree is git's refusal to make, surfaced verbatim.
///
/// # Errors
/// When `gh` is missing, the fetch fails, or git refuses the switch.
pub async fn checkout_pr(repo_path: String, number: u32) -> Result<(), String> {
    super::process::run_blocking(move || {
        let mut cmd = Command::new("gh");
        cmd.args(["pr", "checkout", &number.to_string()]);
        cmd.current_dir(&repo_path);
        super::process::hide_console(&mut cmd);
        let output = super::process::run_timed(cmd, "gh pr checkout", GH_TRANSFER_TIMEOUT)
            .map_err(|e| gh_unavailable(&e))?;
        if !output.status.success() {
            return Err(stderr_or(&output, "gh pr checkout failed."));
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
            return Err(stderr_or(&output, "gh repo clone failed."));
        }
        Ok(target)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping from gh's camelCase JSON to the public shape: the nested
    /// author flattens to a login (a deleted "ghost" author to an empty
    /// string), and gh's empty-string "no review required" normalises to
    /// `None` while a real decision survives.
    #[test]
    fn pr_list_parsing_flattens_author_and_normalises_review_decision() {
        let json = br#"[
            {
                "number": 7,
                "title": "Add thing",
                "state": "OPEN",
                "author": {"login": "leo"},
                "createdAt": "2026-08-01T10:00:00Z",
                "updatedAt": "2026-08-02T10:00:00Z",
                "url": "https://github.com/o/r/pull/7",
                "body": "Details",
                "isDraft": true,
                "baseRefName": "main",
                "headRefName": "feature/thing",
                "reviewDecision": "",
                "additions": 10,
                "deletions": 2,
                "changedFiles": 3
            },
            {
                "number": 8,
                "title": "Ghost PR",
                "state": "MERGED",
                "author": null,
                "reviewDecision": "APPROVED",
                "url": "https://github.com/o/r/pull/8"
            }
        ]"#;

        let prs = parse_pr_list(json).expect("valid gh output parses");
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].number, 7);
        assert_eq!(prs[0].author, "leo");
        assert!(prs[0].is_draft);
        assert_eq!(prs[0].head_ref_name, "feature/thing");
        assert_eq!(
            prs[0].review_decision, None,
            "gh's empty string means no review requirement"
        );
        assert_eq!(prs[0].changed_files, 3);
        assert_eq!(prs[1].author, "", "a ghost author flattens to empty");
        assert_eq!(prs[1].review_decision.as_deref(), Some("APPROVED"));
        assert_eq!(
            prs[1].additions, 0,
            "omitted fields default rather than fail"
        );

        assert!(
            parse_pr_list(b"not json").is_err(),
            "junk is a parse error, not a panic"
        );
    }

    /// The `gh pr checks` contract: parsing is blind to the exit status —
    /// gh exits non-zero whenever a check is pending or failing while still
    /// writing valid JSON — so valid stdout always wins, and only
    /// unparseable stdout falls through to the stderr error path.
    #[test]
    fn pr_checks_parse_stdout_regardless_of_exit_code() {
        let json = br#"[
            {"name": "build", "state": "FAILURE", "bucket": "fail",
             "link": "https://ci/1", "workflow": "CI"},
            {"name": "lint", "state": "IN_PROGRESS", "bucket": "pending"}
        ]"#;

        let checks = parse_pr_checks(json).expect("valid JSON parses even when gh exited 1");
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].bucket, "fail");
        assert_eq!(checks[0].link.as_deref(), Some("https://ci/1"));
        assert_eq!(checks[1].link, None, "omitted link stays None");

        assert!(
            parse_pr_checks(b"").is_none(),
            "empty stdout (no checks at all) falls through to the stderr error"
        );
    }
}

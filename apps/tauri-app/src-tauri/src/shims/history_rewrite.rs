// Thin `#[tauri::command]` delegations to `leogit_core::history_rewrite`.
// Signatures, errors, and semantics are documented on the core functions.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::history_rewrite::{self, RewritePreflight, RewriteResult};

#[tauri::command(async)]
pub fn rewrite_preflight(
    repo_path: String,
    replayed_from: Option<String>,
) -> Result<RewritePreflight, String> {
    history_rewrite::rewrite_preflight(&repo_path, replayed_from.as_deref())
}

#[tauri::command(async)]
pub fn cherry_pick_commits(
    repo_path: String,
    shas: Vec<String>,
    target_branch: String,
) -> Result<RewriteResult, String> {
    history_rewrite::cherry_pick_commits(&repo_path, &shas, &target_branch)
}

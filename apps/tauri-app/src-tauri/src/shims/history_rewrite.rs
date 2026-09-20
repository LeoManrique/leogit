// Thin `#[tauri::command]` delegations to `leogit_core::history_rewrite`.
// Signatures, errors, and semantics are documented on the core functions.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::history_rewrite::{
    self, RewritePreflight, RewriteResult, SquashDraft, UndoPoint, UndoResult,
};

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

#[tauri::command(async)]
pub fn squash_draft(repo_path: String, shas: Vec<String>) -> Result<SquashDraft, String> {
    history_rewrite::squash_draft(&repo_path, &shas)
}

#[tauri::command(async)]
pub fn squash_commits(
    repo_path: String,
    shas: Vec<String>,
    message: String,
) -> Result<RewriteResult, String> {
    history_rewrite::squash_commits(&repo_path, &shas, &message)
}

#[tauri::command(async)]
pub fn reorder_preflight(
    repo_path: String,
    shas: Vec<String>,
    before_sha: Option<String>,
) -> Result<RewritePreflight, String> {
    history_rewrite::reorder_preflight(&repo_path, &shas, before_sha.as_deref())
}

#[tauri::command(async)]
pub fn reorder_commits(
    repo_path: String,
    shas: Vec<String>,
    before_sha: Option<String>,
) -> Result<RewriteResult, String> {
    history_rewrite::reorder_commits(&repo_path, &shas, before_sha.as_deref())
}

#[tauri::command(async)]
pub fn undo_operation(repo_path: String, point: UndoPoint) -> Result<UndoResult, String> {
    history_rewrite::undo_operation(&repo_path, &point)
}

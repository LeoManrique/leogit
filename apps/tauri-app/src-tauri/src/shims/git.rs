// Thin `#[tauri::command]` delegations to `leogit_core::git`. Signatures,
// errors, and semantics are documented on the core functions. The three network
// commands (`pull` / `push` / `clone_repo`) stream `git --progress`, so they
// hand core a `ProgressSink`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::git::{
    self, BranchInfo, CommitDetail, CommitInfo, DiscardPlan, FileEntry, FileStatusStyle,
    LogOptions, MergeResult, RepoIdentifier, RepoStatus, RepoSync,
};
use tauri::AppHandle;

use crate::event_sink::ProgressSink;

// ── Status / inspection ────────────────────────────────────────────────────

/// The letter and name for every `FileStatus` — fetched once at startup, not
/// per row: both clients draw these on every repaint.
#[tauri::command(async)]
pub fn file_status_styles() -> Vec<FileStatusStyle> {
    git::file_status_styles()
}

#[tauri::command(async)]
pub fn get_status(repo_path: String) -> Result<RepoStatus, String> {
    git::get_status(repo_path)
}

#[tauri::command(async)]
pub fn get_selected_diff(repo_path: String, files: Vec<FileEntry>) -> Result<String, String> {
    git::get_selected_diff(repo_path, files)
}

#[tauri::command(async)]
pub fn get_log(repo_path: String, opts: LogOptions) -> Result<Vec<CommitInfo>, String> {
    git::get_log(repo_path, opts)
}

#[tauri::command(async)]
pub fn get_commit_detail(repo_path: String, sha: String) -> Result<CommitDetail, String> {
    git::get_commit_detail(repo_path, sha)
}

// ── Branches ───────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn list_branches(repo_path: String) -> Result<Vec<BranchInfo>, String> {
    git::list_branches(repo_path)
}

#[tauri::command(async)]
pub fn create_branch(repo_path: String, name: String, start_point: String) -> Result<(), String> {
    git::create_branch(repo_path, name, start_point)
}

#[tauri::command(async)]
pub fn switch_branch(repo_path: String, branch: String) -> Result<(), String> {
    git::switch_branch(repo_path, branch)
}

#[tauri::command(async)]
pub fn checkout_commit(repo_path: &str, sha: &str) -> Result<(), String> {
    git::checkout_commit(repo_path, sha)
}

#[tauri::command(async)]
pub fn delete_branch(repo_path: String, name: String) -> Result<(), String> {
    git::delete_branch(repo_path, name)
}

// ── Commits / working tree ─────────────────────────────────────────────────

#[tauri::command(async)]
pub fn commit(
    repo_path: String,
    message: String,
    files: Vec<FileEntry>,
    amend: Option<bool>,
) -> Result<(), String> {
    git::commit(repo_path, message, files, amend)
}

#[tauri::command(async)]
pub fn undo_last_commit(repo_path: String) -> Result<(), String> {
    git::undo_last_commit(repo_path)
}

#[tauri::command(async)]
pub fn classify_discard(repo_path: &str, files: Vec<FileEntry>) -> DiscardPlan {
    git::classify_discard(repo_path, &files)
}

#[tauri::command(async)]
pub fn discard_files(repo_path: &str, files: Vec<FileEntry>) -> Result<(), String> {
    git::discard_files(repo_path, files)
}

#[tauri::command(async)]
pub fn ignore_paths(repo_path: &str, paths: Vec<String>) -> Result<(), String> {
    git::ignore_paths(repo_path, paths)
}

#[tauri::command(async)]
pub fn append_to_gitignore(repo_path: &str, patterns: Vec<String>) -> Result<(), String> {
    git::append_to_gitignore(repo_path, patterns)
}

#[tauri::command]
pub fn format_commit_message(
    summary: String,
    description: String,
    co_authors: Vec<String>,
) -> String {
    git::format_commit_message(summary, description, co_authors)
}

// ── Sync (fetch / pull / push) ─────────────────────────────────────────────

#[tauri::command(async)]
pub fn repo_sync_status(repo_path: String, do_fetch: bool) -> Result<RepoSync, String> {
    git::repo_sync_status(repo_path, do_fetch)
}

#[tauri::command]
pub async fn fetch(repo_path: String, remote: String, background: bool) -> Result<(), String> {
    git::fetch(repo_path, remote, background).await
}

#[tauri::command]
pub async fn pull(app: AppHandle, repo_path: String, remote: String) -> Result<(), String> {
    git::pull(ProgressSink::arc(app), repo_path, remote).await
}

#[tauri::command]
pub async fn push(
    app: AppHandle,
    repo_path: String,
    remote: String,
    branch: String,
    set_upstream: bool,
    force_with_lease: bool,
) -> Result<(), String> {
    git::push(
        ProgressSink::arc(app),
        repo_path,
        remote,
        branch,
        set_upstream,
        force_with_lease,
    )
    .await
}

#[tauri::command(async)]
pub fn get_remote(repo_path: String) -> Result<Option<String>, String> {
    git::get_remote(repo_path)
}

#[tauri::command(async)]
pub fn get_repo_identifier(repo_path: String) -> Option<RepoIdentifier> {
    git::get_repo_identifier(repo_path)
}

// ── Merge ──────────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn merge_branch(repo_path: String, branch: String) -> Result<MergeResult, String> {
    git::merge_branch(repo_path, branch)
}

#[tauri::command(async)]
pub fn merge_squash(repo_path: String, branch: String) -> Result<MergeResult, String> {
    git::merge_squash(repo_path, branch)
}

#[tauri::command(async)]
pub fn commit_squash_merge(repo_path: String) -> Result<(), String> {
    git::commit_squash_merge(repo_path)
}

#[tauri::command(async)]
pub fn merge_abort(repo_path: String) -> Result<(), String> {
    git::merge_abort(repo_path)
}

#[tauri::command(async)]
pub fn count_commits_to_merge(repo_path: String, target_branch: String) -> Result<i32, String> {
    git::count_commits_to_merge(repo_path, target_branch)
}

// ── Clone / discovery / repo lifecycle ─────────────────────────────────────

#[tauri::command]
pub async fn clone_repo(
    app: AppHandle,
    url: String,
    target_path: String,
) -> Result<String, String> {
    git::clone_repo(ProgressSink::arc(app), url, target_path).await
}

#[tauri::command(async)]
pub fn effective_scan_paths(scan_paths: Vec<String>) -> Vec<String> {
    git::effective_scan_paths(scan_paths)
}

#[tauri::command(async)]
pub fn is_git_repo(path: &str) -> bool {
    git::is_git_repo(path)
}

#[tauri::command(async)]
pub fn init_repo(path: &str) -> Result<String, String> {
    git::init_repo(path)
}

// Thin `#[tauri::command]` delegations to `leogit_core::repos`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::process;
use leogit_core::repos::{self, CloneTarget, RepoRow};

/// Async over [`process::run_blocking`], not `#[tauri::command(async)]`: the
/// walk spawns nothing but stats every entry under each scan root, which at the
/// configured depth is tens of thousands of syscalls on a cold page cache. That
/// wait belongs on the blocking pool rather than on a tokio core worker, which
/// is the hop the `SwiftUI` bridge already makes for this call.
#[tauri::command]
pub async fn known_repos(scan_paths: Vec<String>, max_depth: u32) -> Result<Vec<String>, String> {
    process::run_blocking(move || repos::known_repos(scan_paths, max_depth)).await?
}

/// One crossing per keystroke, not one per row — see `repos::filter_repos`.
#[tauri::command(async)]
pub fn filter_repos(query: String, rows: Vec<RepoRow>, scan_folders: Vec<String>) -> Vec<String> {
    repos::filter_repos(&query, &rows, &scan_folders)
}

#[tauri::command(async)]
pub fn derive_clone_target(raw_url: String, parent: String) -> Option<CloneTarget> {
    repos::derive_clone_target(&raw_url, &parent)
}

#[tauri::command(async)]
pub fn clone_target_path(parent: String, repo_name: String) -> Option<String> {
    repos::clone_target_path(&parent, &repo_name)
}

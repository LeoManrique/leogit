// Thin `#[tauri::command]` delegations to `leogit_core::gh`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::gh::{self, GhRepo};
use leogit_core::process;
use tauri::AppHandle;

use crate::event_sink::ProgressSink;

/// `gh auth status` validates the stored token against GitHub, so it carries
/// core's 20 s `gh` budget. Async over [`process::run_blocking`] for the reason
/// spelled out on [`gh_repo_list`]; a join failure means "can't confirm auth",
/// which is the same `false` a spawn failure or a timeout produces.
#[tauri::command]
pub async fn check_auth() -> bool {
    process::run_blocking(gh::check_auth).await.unwrap_or(false)
}

/// Async over [`process::run_blocking`] rather than `#[tauri::command(async)]`:
/// this is a network round trip to GitHub with a 20 s budget, and `(async)` on
/// a sync fn would hold a tokio *core* worker — one of only ~num-cpus — for all
/// of it. The blocking pool exists for exactly this wait.
#[tauri::command]
pub async fn gh_repo_list(limit: u32) -> Result<Vec<GhRepo>, String> {
    process::run_blocking(move || gh::gh_repo_list(limit)).await?
}

#[tauri::command]
pub async fn gh_publish_repo(
    repo_path: String,
    name: String,
    description: String,
    is_private: bool,
) -> Result<(), String> {
    gh::gh_publish_repo(repo_path, name, description, is_private).await
}

#[tauri::command]
pub async fn gh_clone(
    app: AppHandle,
    name_with_owner: String,
    target_path: String,
) -> Result<String, String> {
    gh::gh_clone(ProgressSink::arc(app), name_with_owner, target_path).await
}

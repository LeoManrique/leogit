// Thin `#[tauri::command]` delegations to `leogit_core::gh`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::gh::{self, GhRepo};
use tauri::AppHandle;

use crate::event_sink::TauriEventSink;

#[tauri::command(async)]
pub fn check_auth() -> bool {
    gh::check_auth()
}

#[tauri::command(async)]
pub fn gh_repo_list(limit: u32) -> Result<Vec<GhRepo>, String> {
    gh::gh_repo_list(limit)
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
    gh::gh_clone(TauriEventSink::arc(app), name_with_owner, target_path).await
}

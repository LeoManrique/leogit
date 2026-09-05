// Thin `#[tauri::command]` delegations to `leogit_core::os`.
#![allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]

use leogit_core::{os, process};

// All three hand off to a launcher (Finder / Explorer / `xdg-open`) that only
// returns once the target application has accepted the request — a cold app
// launch, not the microseconds `(async)` on a sync fn assumes — and core bounds
// a wedged one at 15 s. So they go to the blocking pool via
// [`process::run_blocking`] rather than pinning a tokio core worker.

#[tauri::command]
pub async fn reveal_path(repo_path: String, rel_path: String) -> Result<(), String> {
    process::run_blocking(move || os::reveal_path(repo_path, rel_path)).await?
}

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    process::run_blocking(move || os::open_url(url)).await?
}

#[tauri::command]
pub async fn open_path(repo_path: String, rel_path: String) -> Result<(), String> {
    process::run_blocking(move || os::open_path(repo_path, rel_path)).await?
}

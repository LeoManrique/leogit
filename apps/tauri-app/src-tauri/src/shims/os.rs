// Thin `#[tauri::command]` delegations to `leogit_core::os`.
#![allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]

use leogit_core::os;

#[tauri::command(async)]
pub fn reveal_path(repo_path: String, rel_path: String) -> Result<(), String> {
    os::reveal_path(repo_path, rel_path)
}

#[tauri::command(async)]
pub fn open_url(url: String) -> Result<(), String> {
    os::open_url(url)
}

#[tauri::command(async)]
pub fn open_path(repo_path: String, rel_path: String) -> Result<(), String> {
    os::open_path(repo_path, rel_path)
}

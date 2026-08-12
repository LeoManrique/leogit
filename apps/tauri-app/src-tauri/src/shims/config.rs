// Thin `#[tauri::command]` delegations to `leogit_core::config`. Errors and
// semantics are documented on the core functions; duplicating them here would
// only drift.
#![allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]

use leogit_core::config::{self, Config, ReposState, ReposStatePatch};

#[tauri::command]
pub fn load_config() -> Result<Config, String> {
    config::load_config()
}

#[tauri::command]
pub fn save_config(cfg: Config) -> Result<(), String> {
    config::save_config(cfg)
}

#[tauri::command]
pub fn load_state() -> Result<ReposState, String> {
    config::load_state()
}

#[tauri::command]
pub fn patch_state(patch: ReposStatePatch) -> Result<ReposState, String> {
    config::patch_state(patch)
}

#[tauri::command]
pub fn record_recent_repo(path: String) -> Result<ReposState, String> {
    config::record_recent_repo(path)
}

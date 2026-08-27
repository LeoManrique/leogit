// Thin `#[tauri::command]` delegations to `leogit_core::config`. Errors and
// semantics are documented on the core functions; duplicating them here would
// only drift.
#![allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]

use leogit_core::config::{self, Config, ConfigBounds, ConfigPatch, ReposState, ReposStatePatch};

#[tauri::command]
pub fn load_config() -> Result<Config, String> {
    config::load_config()
}

/// The range every numeric setting is clamped to — the form's `min`/`max`,
/// read from the same declaration that enforces them.
#[tauri::command]
#[must_use]
pub fn config_bounds() -> ConfigBounds {
    config::config_bounds()
}

#[tauri::command]
pub fn patch_config(patch: ConfigPatch) -> Result<Config, String> {
    config::patch_config(patch)
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

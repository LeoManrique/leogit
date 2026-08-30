// Thin `#[tauri::command]` delegation to `leogit_core::update`.
#![allow(clippy::missing_errors_doc)]

use leogit_core::update::{self, UpdateInfo};

/// `CARGO_PKG_VERSION` read *here*, in the host crate, is the whole point of
/// core taking the version as an argument: this manifest is the one
/// `deploy_release.py` bumps alongside `tauri.conf.json`, so it is the version
/// this build is released under. Read inside core it would have answered with
/// `leogit-core`'s own, which no release is named after.
#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    update::check_for_update(env!("CARGO_PKG_VERSION")).await
}

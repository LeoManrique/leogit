// Thin `#[tauri::command]` delegation to `leogit_core::update`.
#![allow(clippy::missing_errors_doc)]

use leogit_core::update::{self, UpdateInfo};

#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    update::check_for_update().await
}

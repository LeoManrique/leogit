// Thin `#[tauri::command]` delegation to `leogit_core::launch`. The warm-start
// window glue lives in `crate::launch_glue`; this only exposes the cold-start
// target the frontend claims on mount.
#![allow(clippy::must_use_candidate)]

use leogit_core::launch::{self, LaunchTarget};

#[tauri::command]
pub fn take_pending_launch_target() -> Option<LaunchTarget> {
    launch::take_pending_launch_target()
}

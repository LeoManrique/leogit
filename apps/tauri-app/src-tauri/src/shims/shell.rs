// Thin `#[tauri::command]` delegation to `leogit_core::shell`.
#![allow(clippy::must_use_candidate)]

use leogit_core::shell::{self, ShellOption};

#[tauri::command]
pub fn list_shells() -> Vec<ShellOption> {
    shell::list_shells()
}

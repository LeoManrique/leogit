// Thin `#[tauri::command]` delegation to `leogit_core::highlight`.
#![allow(clippy::needless_pass_by_value, clippy::must_use_candidate)]

use leogit_core::diff::FileDiff;
use leogit_core::highlight::{self, BlobSource};

#[tauri::command(async)]
pub fn highlight_diff(file_diff: FileDiff, source: Option<BlobSource>) -> Vec<String> {
    highlight::highlight_diff(file_diff, source)
}

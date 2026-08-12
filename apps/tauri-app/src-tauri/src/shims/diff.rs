// Thin `#[tauri::command]` delegations to `leogit_core::diff`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::diff::{self, DiffSelection, FileDiff, ParsedDiff};

#[tauri::command(async)]
pub fn parse_diff(raw: String) -> Option<ParsedDiff> {
    diff::parse_diff(raw)
}

#[tauri::command(async)]
pub fn generate_patch(
    repo_path: String,
    file_diff: FileDiff,
    selection: DiffSelection,
) -> Result<(), String> {
    diff::generate_patch(repo_path, file_diff, selection)
}

#[tauri::command(async)]
pub fn generate_inverse_patch(
    repo_path: String,
    file_diff: FileDiff,
    selection: DiffSelection,
) -> Result<(), String> {
    diff::generate_inverse_patch(repo_path, file_diff, selection)
}

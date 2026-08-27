// Thin `#[tauri::command]` delegations to `leogit_core::diff`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::diff::{self, DiffOptions, DiffSelection, FileDiff, ParsedDiff};
use leogit_core::git::FileEntry;

#[tauri::command(async)]
pub fn get_parsed_diff(
    repo_path: String,
    file: FileEntry,
    hide_whitespace: bool,
    options: DiffOptions,
) -> Result<ParsedDiff, String> {
    diff::get_parsed_diff(repo_path, file, hide_whitespace, options)
}

#[tauri::command(async)]
pub fn get_parsed_commit_diff(
    repo_path: String,
    sha: String,
    file_path: String,
    options: DiffOptions,
) -> Result<ParsedDiff, String> {
    diff::get_parsed_commit_diff(repo_path, sha, file_path, options)
}

#[tauri::command(async)]
pub fn copy_diff_text(file_diff: FileDiff, start: u32, end: u32) -> String {
    diff::copy_text(&file_diff, start as usize, end as usize)
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

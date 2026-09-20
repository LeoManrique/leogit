// Thin `#[tauri::command]` delegations to `leogit_core::operation`. Signatures,
// errors, and semantics are documented on the core functions.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::operation::{self, OperationOutcome};

#[tauri::command(async)]
pub fn continue_operation(repo_path: String) -> Result<OperationOutcome, String> {
    operation::continue_operation(&repo_path)
}

#[tauri::command(async)]
pub fn abort_operation(repo_path: String) -> Result<Option<String>, String> {
    operation::abort_operation(&repo_path)
}

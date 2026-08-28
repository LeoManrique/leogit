// Thin `#[tauri::command]` delegation to `leogit_core::exclusions`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::exclusions::{self, Exclusion};

/// One crossing per status tick, and only while the user has actually excluded
/// something — the client short-circuits an empty set rather than asking core
/// to reconcile nothing. See `exclusions::reconcile_exclusions` for the rule.
#[tauri::command(async)]
pub fn reconcile_exclusions(
    excluded: Vec<Exclusion>,
    present: Vec<String>,
    elapsed_ms: u32,
) -> Vec<Exclusion> {
    exclusions::reconcile_exclusions(&excluded, &present, elapsed_ms)
}

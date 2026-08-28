//! `#[tauri::command]` entry points, one per core function.
//!
//! Every command here is a 1:1 delegation to `leogit_core`: the real
//! signatures, error conditions, and semantics live there. The command macro
//! must annotate the definition site (it generates the handler `generate_handler!`
//! wires up), which is why these thin wrappers exist on the host rather than the
//! core function being registered directly.

pub mod ai;
pub mod config;
pub mod diff;
pub mod exclusions;
pub mod gh;
pub mod git;
pub mod highlight;
pub mod launch;
pub mod os;
pub mod repos;
pub mod shell;
pub mod terminal;
pub mod update;

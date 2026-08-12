// Thin `#[tauri::command]` delegations to `leogit_core::terminal`.
// `start_terminal` streams PTY output, so it hands core a `TauriEventSink`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::terminal::{self, PtyInfo, StartedTerminal};
use tauri::AppHandle;

use crate::event_sink::TauriEventSink;

#[tauri::command]
pub fn terminal_pty_info() -> PtyInfo {
    terminal::terminal_pty_info()
}

#[tauri::command]
pub fn start_terminal(
    app: AppHandle,
    repo_path: &str,
    shell_id: Option<String>,
) -> Result<StartedTerminal, String> {
    terminal::start_terminal(TauriEventSink::arc(app), repo_path, shell_id)
}

#[tauri::command]
pub fn write_terminal(pid: u32, data: &str) -> Result<(), String> {
    terminal::write_terminal(pid, data)
}

#[tauri::command]
pub fn resize_terminal(pid: u32, cols: u16, rows: u16) -> Result<(), String> {
    terminal::resize_terminal(pid, cols, rows)
}

#[tauri::command]
pub fn close_terminal(pid: u32) -> Result<(), String> {
    terminal::close_terminal(pid)
}

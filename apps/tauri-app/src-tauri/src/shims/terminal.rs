// Thin `#[tauri::command]` delegations to `leogit_core::terminal`.
// `start_terminal` streams PTY output, so it takes the frontend's channel and
// hands core a `TerminalChannelSink` over it.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use leogit_core::terminal::{self, PtyInfo, StartedTerminal};
use tauri::ipc::Channel;

use crate::event_sink::{TerminalChannelSink, TerminalEvent};

#[tauri::command]
pub fn terminal_pty_info() -> PtyInfo {
    terminal::terminal_pty_info()
}

// Opens a PTY and spawns a shell, both of which touch the filesystem, so it
// runs off the main thread like every other subprocess command.
#[tauri::command(async)]
pub fn start_terminal(
    on_event: Channel<TerminalEvent>,
    repo_path: String,
    shell_id: Option<String>,
) -> Result<StartedTerminal, String> {
    terminal::start_terminal(TerminalChannelSink::arc(on_event), &repo_path, shell_id)
}

// Deliberately *not* `(async)`: this is the keystroke path, and a sync command
// runs inline in the order its IPC messages arrive. Spawning each write onto
// the async runtime would let two keystrokes race, which is the one thing a
// terminal may never do. The write itself is a `write_all` to a PTY master —
// microseconds, and the main thread is where the keystroke already is.
//
// That "microseconds" is only true because the session's mutex is never held
// across anything slow: core keeps the child process behind a second lock of
// its own precisely so `close_terminal`'s ~250 ms kill cannot end up in front
// of a keystroke on this thread.
#[tauri::command]
pub fn write_terminal(pid: u32, data: &str) -> Result<(), String> {
    terminal::write_terminal(pid, data)
}

// Takes the session mutex, and a PTY resize is an ioctl on a handle the reader
// thread is blocked on; off the main thread so nothing about a teardown in
// flight can hitch the window. Ordering does not matter here — a resize is a
// level, not an edge, and the frontend debounces a drag down to its final size
// before sending one.
#[tauri::command(async)]
pub fn resize_terminal(pid: u32, cols: u16, rows: u16) -> Result<(), String> {
    terminal::resize_terminal(pid, cols, rows)
}

// `portable-pty`'s kill escalates SIGHUP → grace loop → SIGKILL and can block
// ~250 ms. On the main thread that is a visible hitch on every panel close.
#[tauri::command(async)]
pub fn close_terminal(pid: u32) -> Result<(), String> {
    terminal::close_terminal(pid)
}

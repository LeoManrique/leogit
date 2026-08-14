//! Maps [`leogit_core::events::CoreEvent`]s onto Tauri window events.
//!
//! Core's streaming producers (git `--progress`, PTY output) hold an
//! `Arc<dyn EventSink>`; this is the Tauri implementation. Each variant emits
//! the exact channel + payload the Svelte frontend already listens for, so the
//! extraction is invisible to the UI.

use std::sync::Arc;

use leogit_core::events::{CoreEvent, EventSink};
use leogit_core::git::GIT_PROGRESS_EVENT;
use tauri::{AppHandle, Emitter};

/// A [`leogit_core::events::EventSink`] backed by a Tauri [`AppHandle`].
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    /// Wrap `app` as the `Arc<dyn EventSink>` core's streaming commands expect.
    #[must_use]
    pub fn arc(app: AppHandle) -> Arc<dyn EventSink> {
        Arc::new(Self { app })
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: CoreEvent) {
        match event {
            // `GitProgress` derives `Serialize` with exactly the fields the
            // frontend reads (`op` / `path` / `percent` / `text`).
            CoreEvent::GitProgress(progress) => {
                let _ = self.app.emit(GIT_PROGRESS_EVENT, progress);
            }
            CoreEvent::TerminalOutput { pid, data } => {
                let _ = self.app.emit(&format!("terminal-output-{pid}"), data);
            }
            // `TerminalExit` derives `Serialize` with the fields the frontend
            // reads (`exit_code` / `signal`).
            CoreEvent::TerminalClosed { pid, exit } => {
                let _ = self.app.emit(&format!("terminal-closed-{pid}"), exit);
            }
        }
    }
}

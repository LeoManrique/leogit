//! The one seam between UI-framework-agnostic core logic and its host.
//!
//! A few operations are *streaming*: git network commands report `--progress`
//! lines while they run, and a terminal PTY emits output continuously until it
//! closes. Rather than let those producers reach for a `tauri::AppHandle`
//! directly (which would drag Tauri into the core), they deliver [`CoreEvent`]s
//! through a host-supplied [`EventSink`]. The Tauri host maps each event onto a
//! window `emit`; a future SwiftUI/UniFFI host maps them onto a Swift callback.
//!
//! Everything non-streaming stays a plain `Result<T, String>` return and never
//! touches this module.

use serde::Serialize;

/// Payload of a git network-progress tick (push / pull / clone). Serialized
/// verbatim by the Tauri host onto the `git-progress` event, so its field names
/// are load-bearing — the frontend reads `op` / `path` / `percent` / `text`.
#[derive(Debug, Clone, Serialize)]
pub struct GitProgress {
    /// Which operation is reporting: `"push"`, `"pull"`, or `"clone"`.
    pub op: &'static str,
    /// Repo path (clone target for `"clone"`), so listeners can filter.
    pub path: String,
    /// Aggregate progress across the operation's phases, 0–100.
    pub percent: f32,
    /// Raw git progress line, e.g. `"Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s"`.
    pub text: String,
}

/// How a terminal session's child ended — the payload of
/// [`CoreEvent::TerminalClosed`], collected by `child.wait()` after the PTY
/// reaches EOF. Serialized verbatim by the Tauri host, so the field names are
/// load-bearing.
///
/// The UIs key off it: a clean exit closes the panel, anything else keeps the
/// dead terminal on screen with the reason, so a shell that dies instantly (a
/// broken `.zshrc`) no longer flashes its error away.
#[derive(Debug, Clone, Serialize)]
pub struct TerminalExit {
    /// The child's exit code. When the child died to a signal this is a
    /// fabricated `1` — the real information is in `signal`.
    pub exit_code: u32,
    /// Name of the fatal signal (`"Hangup"` after a kill via the panel's ✕,
    /// since the escalation starts with SIGHUP), `None` on a normal exit.
    pub signal: Option<String>,
}

impl TerminalExit {
    /// Whether the child ended by exiting zero — the one case the UIs treat
    /// as "nothing to report" and close the panel on.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.exit_code == 0 && self.signal.is_none()
    }
}

/// An event core wants delivered to the host UI. Each variant carries exactly
/// the data the host needs to reproduce today's Tauri payloads.
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// A git network-progress tick. Host → `git-progress`.
    GitProgress(GitProgress),
    /// A chunk of decoded terminal output for session `pid`.
    /// Host → `terminal-output-{pid}`.
    TerminalOutput { pid: u32, data: String },
    /// Terminal session `pid` reached EOF and was torn down; `exit` is the
    /// reaped child's status. Host → `terminal-closed-{pid}`.
    TerminalClosed { pid: u32, exit: TerminalExit },
}

/// Host-provided delivery channel for [`CoreEvent`]s.
///
/// `Send + Sync` so streaming producers can hold it across threads and tasks:
/// the PTY reader thread and the git `--progress` callback both keep an
/// `Arc<dyn EventSink>` for the lifetime of the operation.
pub trait EventSink: Send + Sync {
    /// Deliver one event. Implementations must not block or panic — a dropped
    /// event should degrade the UI (a stalled progress bar), never the op.
    fn emit(&self, event: CoreEvent);
}

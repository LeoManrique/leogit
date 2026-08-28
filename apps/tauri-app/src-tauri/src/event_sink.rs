//! Maps [`leogit_core::events::CoreEvent`]s onto this host's transports.
//!
//! Core's streaming producers (git `--progress`, PTY output) hold an
//! `Arc<dyn EventSink>`; these are the Tauri implementations. There are two,
//! scoped exactly the way the UniFFI bridge's are, because the two streams
//! want opposite things from a transport:
//!
//! * **Git progress** is a broadcast about a named repository. Any part of the
//!   UI may want it, the payload carries the `path` listeners filter on, and a
//!   dropped tick costs a stalled bar — so it stays a window event.
//! * **Terminal output** belongs to exactly one panel, must not be dropped, and
//!   starts arriving before the command that started the session has returned.
//!   A window event cannot express that: the frontend can only subscribe *after*
//!   it learns the pid, and the reader thread is already emitting into the gap.
//!   So the session's stream rides a [`Channel`] the frontend creates and hands
//!   in — the listener exists before core can hold it, which is what closes the
//!   race rather than narrowing it.
//!
//! Each sink ignores the variants its operation cannot emit, so neither has to
//! know the other exists.

use std::sync::Arc;

use leogit_core::events::{CoreEvent, EventSink, TerminalExit};
use leogit_core::git::GIT_PROGRESS_EVENT;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};

/// An [`EventSink`] for the git network commands, backed by a Tauri
/// [`AppHandle`] and emitting on the app-wide `git-progress` event.
pub struct ProgressSink {
    app: AppHandle,
}

impl ProgressSink {
    /// Wrap `app` as the `Arc<dyn EventSink>` core's streaming git commands
    /// expect.
    #[must_use]
    pub fn arc(app: AppHandle) -> Arc<dyn EventSink> {
        Arc::new(Self { app })
    }
}

impl EventSink for ProgressSink {
    fn emit(&self, event: CoreEvent) {
        // `GitProgress` derives `Serialize` with exactly the fields the
        // frontend reads (`op` / `path` / `percent` / `text`). The terminal
        // variants are ignored: this sink is only ever handed to push / pull /
        // clone, whose producers cannot emit them.
        if let CoreEvent::GitProgress(progress) = event {
            let _ = self.app.emit(GIT_PROGRESS_EVENT, progress);
        }
    }
}

/// One terminal session's stream, as the frontend receives it.
///
/// The session *is* the channel, so nothing here carries a pid — which is the
/// point: output can arrive before `start_terminal` has returned one, and a
/// payload that needed it would be unreadable exactly when it matters most.
///
/// Serialized internally tagged, so the frontend reads a discriminated union
/// on `event`. [`TerminalExit`]'s own field names (`exit_code`, `signal`) come
/// from its derive and are load-bearing.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TerminalEvent {
    /// A chunk of decoded PTY output, already coalesced by core.
    Output { data: String },
    /// The child exited and was reaped; nothing further will arrive.
    Closed { exit: TerminalExit },
}

/// An [`EventSink`] for one PTY session, backed by the frontend's [`Channel`].
///
/// `Channel` is `Send + Sync + Clone` and stamps every send with a monotonic
/// index the JS side reorders on, so core's reader and emitter threads can hold
/// this for the life of the session and output arrives in the order it was
/// produced.
pub struct TerminalChannelSink {
    channel: Channel<TerminalEvent>,
}

impl TerminalChannelSink {
    /// Wrap the frontend's channel as the `Arc<dyn EventSink>`
    /// `start_terminal` expects.
    #[must_use]
    pub fn arc(channel: Channel<TerminalEvent>) -> Arc<dyn EventSink> {
        Arc::new(Self { channel })
    }
}

impl EventSink for TerminalChannelSink {
    fn emit(&self, event: CoreEvent) {
        // Git progress is ignored: this sink is only ever handed to
        // `start_terminal`, whose producers cannot emit it.
        let message = match event {
            CoreEvent::TerminalOutput { data, .. } => TerminalEvent::Output { data },
            CoreEvent::TerminalClosed { exit, .. } => TerminalEvent::Closed { exit },
            CoreEvent::GitProgress(_) => return,
        };
        // A failed send means the webview is gone — the panel it was feeding
        // went with it, so there is nothing to report and nothing to retry.
        let _ = self.channel.send(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend reads this stream as a hand-written discriminated union on
    /// `event`, and nothing between the two languages checks that they agree —
    /// so the tag, the variant names and `TerminalExit`'s own field names are
    /// all contract. Changing any of them here without changing
    /// `api/commands.ts` breaks the panel silently at runtime.
    #[test]
    fn output_serializes_as_the_frontend_reads_it() {
        let json = serde_json::to_string(&TerminalEvent::Output {
            data: "hello\r\n".to_string(),
        })
        .expect("serialize");
        assert_eq!(json, r#"{"event":"output","data":"hello\r\n"}"#);
    }

    #[test]
    fn closed_carries_the_exit_fields_unwrapped() {
        let json = serde_json::to_string(&TerminalEvent::Closed {
            exit: TerminalExit {
                exit_code: 1,
                signal: Some("Hangup".to_string()),
            },
        })
        .expect("serialize");
        assert_eq!(
            json,
            r#"{"event":"closed","exit":{"exit_code":1,"signal":"Hangup"}}"#
        );
    }
}

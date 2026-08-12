//! PTY-backed shell sessions for the embedded terminal panel.
//!
//! Each session owns a `portable-pty` master (`ConPTY` on Windows, a Unix pty
//! elsewhere), a reader thread that pumps output to the frontend as
//! `terminal-output-<pid>` events, and the child shell process. Sessions are
//! keyed by a synthetic id rather than the OS pid so the frontend never holds a
//! reusable process handle.
//!
//! ## Environment
//!
//! `CommandBuilder::new` already assembles the correct child environment: it
//! seeds from this process's environment and then, on Windows, overlays the
//! authoritative `HKLM`/`HKCU` `Environment` keys — so `PATH` becomes the same
//! system+user merge that Explorer and Windows Terminal hand their children.
//! We deliberately do **not** copy our own environment over the top of that.
//! Doing so used to replace that merged `PATH` with whatever the parent process
//! happened to hold, which broke the terminal two ways: launched from Git Bash
//! the inherited `PATH` is MSYS-style (`/usr/bin`, `/c/Program Files/...`),
//! which Win32 cannot resolve at all, and launched from Explorer it is a stale
//! login-time snapshot that misses anything installed since. Only deliberate
//! additions are set below.

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

use super::paths;
use super::shell::{self, ShellOption};
use crate::events::{CoreEvent, EventSink};

/// Holds the master PTY (must stay alive to keep the PTY open),
/// its writer (for stdin), and the child process handle.
struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

static SESSIONS: LazyLock<Mutex<HashMap<u32, Arc<Mutex<PtySession>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

/// Streaming UTF-8 decoder for PTY output.
///
/// PTY reads are byte-oriented and split wherever the buffer happens to fill,
/// so a multi-byte character routinely straddles two reads. Decoding each chunk
/// independently with `String::from_utf8_lossy` turns every one of those splits
/// into a permanent replacement character — visible as stray U+FFFD marks in
/// long output, box-drawing, accented text and emoji. Holding the truncated
/// tail until its continuation bytes arrive is what conhost and Windows
/// Terminal do, and it is what makes non-ASCII output survive intact.
///
/// `partial` only ever retains an incomplete trailing sequence, so it is
/// bounded at three bytes between calls and cannot grow without limit.
#[derive(Default)]
struct Utf8Decoder {
    partial: Vec<u8>,
}

impl Utf8Decoder {
    /// Decode as much of `chunk` (plus any held-over tail) as forms complete
    /// characters, retaining a truncated trailing sequence for the next call.
    fn push(&mut self, chunk: &[u8]) -> String {
        self.partial.extend_from_slice(chunk);
        let mut out = String::with_capacity(self.partial.len());
        loop {
            match std::str::from_utf8(&self.partial) {
                Ok(text) => {
                    out.push_str(text);
                    self.partial.clear();
                    return out;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    // `valid_up_to` guarantees this prefix is well-formed.
                    out.push_str(std::str::from_utf8(&self.partial[..valid]).unwrap_or_default());
                    match err.error_len() {
                        // Truncated sequence at the end of the buffer: keep it
                        // and finish decoding once the rest of it arrives.
                        None => {
                            self.partial.drain(..valid);
                            return out;
                        }
                        // Genuinely invalid bytes: emit one replacement
                        // character and resume after them, as lossy would.
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            self.partial.drain(..valid + bad);
                        }
                    }
                }
            }
        }
    }
}

/// What the frontend needs to configure xterm.js before it opens a buffer.
///
/// xterm reads `windowsPty` when it constructs the buffer, so this has to be
/// fetched *before* the terminal is created rather than returned from
/// [`start_terminal`]. Telling xterm it is talking to `ConPTY` on a build that
/// supports it (>= 21376) is what enables reflow on resize; without it xterm
/// falls back to a heuristic that treats any line whose last cell is non-blank
/// as wrapped, which is what makes a resized PowerShell prompt smear.
#[derive(Debug, Clone, Serialize)]
pub struct PtyInfo {
    /// `"conpty"` on Windows, `None` elsewhere (xterm only uses this on Windows).
    pub backend: Option<String>,
    /// Windows build number, e.g. `26200`.
    pub build_number: Option<u32>,
}

/// Read the Windows build number from the registry. `CurrentBuildNumber` is a
/// `REG_SZ`, and is the value Windows itself reports — unlike `GetVersionEx`,
/// it is not subject to application compatibility shimming.
#[cfg(windows)]
fn windows_build_number() -> Option<u32> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?
        .get_value::<String, _>("CurrentBuildNumber")
        .ok()?
        .parse()
        .ok()
}

/// Describe the PTY backend so the frontend can configure xterm.js correctly.
#[must_use]
pub fn terminal_pty_info() -> PtyInfo {
    #[cfg(windows)]
    {
        // portable-pty requires ConPTY (Windows 10 1809+) and panics at startup
        // otherwise, so reaching here means ConPTY is the backend.
        PtyInfo {
            backend: Some("conpty".to_string()),
            build_number: windows_build_number(),
        }
    }
    #[cfg(not(windows))]
    {
        PtyInfo {
            backend: None,
            build_number: None,
        }
    }
}

/// A freshly started session: the handle for subsequent commands, plus the
/// shell that was actually launched so the panel can label itself. The label
/// is resolved here rather than by the frontend because the stored preference
/// may name a shell that is no longer installed.
#[derive(Debug, Clone, Serialize)]
pub struct StartedTerminal {
    pub pid: u32,
    pub shell_id: String,
    pub shell_label: String,
}

/// The environment additions a session needs on top of the base environment
/// `CommandBuilder` assembled (see module docs). Kept pure so the policy is
/// testable without spawning anything.
fn session_env(chosen: &ShellOption) -> Vec<(&'static str, &'static str)> {
    // Advertise what xterm.js actually is. Tools gate colour output on these.
    let mut env = vec![("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

    // MSYS profiles `cd` to $HOME unless this is set. Git for Windows' own
    // /etc/profile currently does not, but MSYS2's does, and honouring it is
    // what guarantees the shell starts in the repo we opened rather than the
    // user's home directory.
    if chosen.id == "git-bash" {
        env.push(("CHERE_INVOKING", "1"));
    }

    env
}

/// Start a new PTY shell with `cwd = repo_path`, running the shell named by
/// `shell_id` (falling back to the best available one when it is `None` or no
/// longer installed).
///
/// # Errors
/// Returns `Err` if the PTY cannot be opened or the shell cannot be spawned.
// `shell_id` is owned because the host deserializes command arguments into
// owned values; a borrowed `Option<&str>` would tie the signature to the
// request's lifetime for no gain.
#[allow(clippy::needless_pass_by_value)]
pub fn start_terminal(
    sink: Arc<dyn EventSink>,
    repo_path: &str,
    shell_id: Option<String>,
) -> Result<StartedTerminal, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let chosen = shell::resolve(shell_id.as_deref());
    // The shell is a third-party program reading this path, and Windows
    // verbatim paths (`\\?\C:\…`) are exactly what they mishandle: PowerShell
    // can't map one onto a PSDrive, so it drops to a provider-qualified prompt
    // (`PS Microsoft.PowerShell.Core\FileSystem::\\?\C:\…`) and every script
    // that touches `$PWD` sees that instead of a path. Repo paths already
    // arrive converted (see `commands::paths`); converting again here costs a
    // string compare and keeps the guarantee local to the process boundary
    // that actually depends on it. No-op off Windows.
    let cwd = paths::simplify_str(repo_path);
    println!(
        "[terminal] launching {} ({}) in {cwd}",
        chosen.label, chosen.path
    );

    let mut cmd = CommandBuilder::new(&chosen.path);
    cmd.args(&chosen.args);
    cmd.cwd(&cwd);
    for (key, value) in session_env(&chosen) {
        cmd.env(key, value);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| {
        let msg = format!("failed to launch {}: {e}", chosen.label);
        eprintln!("[terminal] {msg}");
        msg
    })?;

    // Close slave fd in this process; child has its own copy.
    drop(pair.slave);

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    let session = Arc::new(Mutex::new(PtySession {
        master: pair.master,
        writer,
        child,
    }));
    SESSIONS
        .lock()
        .map_err(|_| "sessions lock poisoned")?
        .insert(pid, session);

    // Reader thread: pump PTY output through the event sink, scoped by pid.
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut decoder = Utf8Decoder::default();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = decoder.push(&buf[..n]);
                    if !data.is_empty() {
                        sink.emit(CoreEvent::TerminalOutput { pid, data });
                    }
                }
                Err(e) => {
                    // EOF surfaces as EIO on Linux once the slave closes; that
                    // is a normal exit, not a fault worth logging loudly.
                    println!("[terminal] session {pid} read ended: {e}");
                    break;
                }
            }
        }
        // Clean up session on EOF/error and notify frontend. A poisoned lock
        // means another session's teardown panicked; the frontend still needs
        // its close event, so don't unwind this thread over it.
        if let Ok(mut sessions) = SESSIONS.lock() {
            sessions.remove(&pid);
        }
        sink.emit(CoreEvent::TerminalClosed { pid });
    });

    Ok(StartedTerminal {
        pid,
        shell_id: chosen.id,
        shell_label: chosen.label,
    })
}

/// Look up a live session by id, releasing the map lock before the caller
/// touches the session itself.
fn session_for(pid: u32) -> Result<Arc<Mutex<PtySession>>, String> {
    let sessions = SESSIONS.lock().map_err(|_| "sessions lock poisoned")?;
    sessions
        .get(&pid)
        .ok_or_else(|| format!("no pty for pid {pid}"))
        .cloned()
}

/// Write data to PTY stdin.
///
/// # Errors
/// Returns `Err` if the session is gone or the write fails.
pub fn write_terminal(pid: u32, data: &str) -> Result<(), String> {
    let session = session_for(pid)?;
    let mut s = session.lock().map_err(|_| "session lock poisoned")?;
    s.writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    s.writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Resize the PTY window.
///
/// # Errors
/// Returns `Err` if the session is gone or the resize fails.
pub fn resize_terminal(pid: u32, cols: u16, rows: u16) -> Result<(), String> {
    let session = session_for(pid)?;
    let s = session.lock().map_err(|_| "session lock poisoned")?;
    s.master
        .resize(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Kill the child process and remove the session.
///
/// # Errors
/// Returns `Err` only if the session map lock is poisoned.
pub fn close_terminal(pid: u32) -> Result<(), String> {
    let mut sessions = SESSIONS.lock().map_err(|_| "sessions lock poisoned")?;
    if let Some(session) = sessions.remove(&pid)
        && let Ok(mut s) = session.lock()
    {
        let _ = s.child.kill();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `input` to the decoder one `size`-byte chunk at a time.
    fn decode_in_chunks(input: &[u8], size: usize) -> String {
        let mut decoder = Utf8Decoder::default();
        let mut out = String::new();
        for chunk in input.chunks(size) {
            out.push_str(&decoder.push(chunk));
        }
        out
    }

    #[test]
    fn multi_byte_chars_survive_every_chunk_boundary() {
        // Accented Latin (2 bytes), box drawing (3 bytes), emoji (4 bytes).
        let text = "árbol ├── café 🚀 ünïcödé";
        for size in 1..=text.len() {
            assert_eq!(
                decode_in_chunks(text.as_bytes(), size),
                text,
                "corrupted when split into {size}-byte chunks"
            );
        }
    }

    #[test]
    fn holds_truncated_tail_until_it_completes() {
        let mut decoder = Utf8Decoder::default();
        let bytes = "é".as_bytes(); // 0xC3 0xA9
        assert_eq!(
            decoder.push(&bytes[..1]),
            "",
            "must not emit a partial char"
        );
        assert_eq!(decoder.push(&bytes[1..]), "é");
    }

    #[test]
    fn invalid_bytes_become_one_replacement_and_decoding_continues() {
        let mut decoder = Utf8Decoder::default();
        // 0xFF is never valid UTF-8; the surrounding text must still arrive.
        assert_eq!(decoder.push(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn partial_buffer_stays_bounded_across_many_chunks() {
        let mut decoder = Utf8Decoder::default();
        for _ in 0..1000 {
            decoder.push("🚀".as_bytes());
            // A held-over tail is at most the 3 continuation bytes of a 4-byte
            // sequence, so the buffer can never accumulate.
            assert!(decoder.partial.len() < 4);
        }
    }

    #[test]
    fn pty_info_reports_conpty_on_windows_only() {
        let info = terminal_pty_info();
        if cfg!(windows) {
            assert_eq!(info.backend.as_deref(), Some("conpty"));
            // Reflow support needs a real build number; ConPTY itself requires
            // 17763, so anything lower means we failed to read the registry.
            assert!(info.build_number.is_none_or(|b| b >= 17763));
        } else {
            assert!(info.backend.is_none());
        }
    }

    fn shell_with_id(id: &str) -> ShellOption {
        ShellOption {
            id: id.to_string(),
            label: id.to_string(),
            path: "/bin/sh".to_string(),
            args: Vec::new(),
        }
    }

    #[test]
    fn every_session_advertises_a_colour_capable_terminal() {
        for id in ["git-bash", "pwsh", "windows-powershell", "cmd", "default"] {
            let env = session_env(&shell_with_id(id));
            assert!(env.contains(&("TERM", "xterm-256color")), "{id} lacks TERM");
            assert!(
                env.contains(&("COLORTERM", "truecolor")),
                "{id} lacks COLORTERM"
            );
        }
    }

    #[test]
    fn only_git_bash_opts_into_chere_invoking() {
        let has_chere = |id: &str| {
            session_env(&shell_with_id(id))
                .iter()
                .any(|(k, _)| *k == "CHERE_INVOKING")
        };
        assert!(has_chere("git-bash"), "git-bash must keep the PTY cwd");
        for id in ["pwsh", "windows-powershell", "cmd", "default"] {
            assert!(!has_chere(id), "{id} must not get an MSYS-only variable");
        }
    }

    /// The regression this module's docs are about: nothing here may copy the
    /// parent environment over the base `CommandBuilder` assembled, because on
    /// Windows that replaces the registry-merged PATH with a stale or
    /// MSYS-style one.
    #[test]
    fn session_env_never_overrides_path() {
        for id in ["git-bash", "pwsh", "windows-powershell", "cmd", "default"] {
            assert!(
                !session_env(&shell_with_id(id))
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("path")),
                "{id} must inherit PATH, not set it"
            );
        }
    }
}

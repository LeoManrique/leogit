//! Cross-platform subprocess spawning helpers.
//!
//! Release builds run as a GUI app (`windows_subsystem = "windows"`), which on
//! Windows has no attached console. When such a process spawns a console
//! subprocess (`git`, `gh`, `claude`), Windows allocates and briefly flashes a
//! new console window for each call. Because the UI polls `git status` every
//! 2s, that means a `cmd` box flickering on screen continuously.
//!
//! Passing the `CREATE_NO_WINDOW` creation flag suppresses that window. These
//! helpers are no-ops on every non-Windows platform, so call sites stay
//! platform-agnostic.
//!
//! This module also owns [`run_timed`], the bounded subprocess runner every
//! network-touching git/gh command goes through so an offline or flaky
//! connection can never wedge a worker thread indefinitely.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// CREATE_NO_WINDOW process creation flag (Win32 `winbase.h`).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Suppress the console window for a std `Command` on Windows. Returns the
/// command for chaining; no-op on other platforms.
pub fn hide_console(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Suppress the console window for a tokio `Command` on Windows. Returns the
/// command for chaining; no-op on other platforms.
pub fn hide_console_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// How often the timeout loop wakes to check whether the child has exited.
/// Small enough to react promptly, large enough not to busy-spin.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Callback fed each stderr line as it arrives (see [`run_timed_streaming`]).
type StderrLineFn = Box<dyn FnMut(&str) + Send>;

/// Run a fully blocking task on tokio's dedicated blocking pool and await it.
///
/// A `#[tauri::command(async)]` sync fn runs inline inside a spawned future on
/// a tokio *core* worker (one of ~num-cpus) — fine for a quick subprocess, but
/// a transfer that can legitimately run for minutes (push, pull, clone) would
/// pin a core worker for its whole duration and, on a low-core machine, starve
/// every other command. The blocking pool exists precisely for such tasks, so
/// the long-running network commands are `async fn`s that delegate here.
///
/// # Errors
/// When the blocking task panics or the runtime is shutting down.
pub async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Run `cmd` to completion or until `timeout` elapses, whichever comes first,
/// returning its [`Output`] (status + captured stdout/stderr).
///
/// This is the single chokepoint that keeps a network subprocess (`git fetch`,
/// `gh repo list`, …) from blocking a worker thread forever when the machine is
/// offline or the connection is flaky: if the child outlives `timeout` it is
/// killed and an `Err` is returned, tagged with `label` so the caller can treat
/// it as a transient/offline condition.
///
/// Both pipes are drained on helper threads so a chatty child (git progress on
/// stderr) can never deadlock by filling a pipe buffer while we wait for it.
///
/// # Errors
/// Returns `Err` if the process fails to spawn, can't be waited on, or exceeds
/// `timeout` (in which case it is killed first). A non-zero exit is *not* an
/// error — inspect [`Output::status`] for that.
pub fn run_timed(cmd: Command, label: &str, timeout: Duration) -> Result<Output, String> {
    run_timed_inner(cmd, label, timeout, None)
}

/// [`run_timed`], but each line the child writes to stderr is also handed to
/// `on_stderr_line` as it arrives — the transport for live `git --progress`
/// output. Git repaints its progress meter with carriage returns, so "lines"
/// are split on `\r` as well as `\n`; the full stderr is still captured in the
/// returned [`Output`] for error reporting.
///
/// The callback runs on the stderr reader thread, so it must not block: a slow
/// callback would stall the pipe drain this function exists to guarantee.
///
/// # Errors
/// Same contract as [`run_timed`].
pub fn run_timed_streaming(
    cmd: Command,
    label: &str,
    timeout: Duration,
    on_stderr_line: impl FnMut(&str) + Send + 'static,
) -> Result<Output, String> {
    run_timed_inner(cmd, label, timeout, Some(Box::new(on_stderr_line)))
}

fn run_timed_inner(
    mut cmd: Command,
    label: &str,
    timeout: Duration,
    on_stderr_line: Option<StderrLineFn>,
) -> Result<Output, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{label}: {e}"))?;

    // Move the pipe ends onto reader threads so buffering can't stall the child.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = std::thread::spawn(move || match on_stderr_line {
        None => {
            let mut buf = Vec::new();
            if let Some(p) = err_pipe.as_mut() {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        }
        Some(mut on_line) => {
            // Incremental drain: accumulate everything (the caller still gets
            // full stderr on failure) while feeding each complete line to the
            // callback the moment its terminator arrives.
            let mut all = Vec::new();
            let mut line_start = 0usize;
            let mut chunk = [0u8; 8192];
            if let Some(p) = err_pipe.as_mut() {
                loop {
                    match p.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            all.extend_from_slice(&chunk[..n]);
                            while let Some(rel) = all[line_start..]
                                .iter()
                                .position(|&b| b == b'\n' || b == b'\r')
                            {
                                let end = line_start + rel;
                                on_line(&String::from_utf8_lossy(&all[line_start..end]));
                                line_start = end + 1;
                            }
                        }
                    }
                }
                if line_start < all.len() {
                    on_line(&String::from_utf8_lossy(&all[line_start..]));
                }
            }
            all
        }
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    break None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(format!("{label}: {e}")),
        }
    };

    if status.is_none() {
        // Killing the child closes its pipe write ends, so the reader threads'
        // `read_to_end` returns and the joins below don't hang.
        let _ = child.kill();
        let _ = child.wait();
    }

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();

    match status {
        Some(status) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        None => Err(format!(
            "{label} timed out after {}s (network unreachable?)",
            timeout.as_secs()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_timed_captures_a_quick_command() {
        let mut cmd = Command::new("git");
        cmd.arg("--version");
        let out = run_timed(cmd, "git --version", Duration::from_secs(10)).expect("git --version");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("git version"));
    }

    // Git repaints its progress meter with bare carriage returns, so the
    // streaming reader must treat `\r` as a line break, deliver the trailing
    // unterminated chunk, and still hand back the byte-exact stderr capture.
    #[cfg(unix)]
    #[test]
    fn run_timed_streaming_splits_stderr_on_cr_and_lf() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'a\\rb\\nc' 1>&2"]);
        let (tx, rx) = std::sync::mpsc::channel();
        let out = run_timed_streaming(cmd, "sh", Duration::from_secs(10), move |line| {
            let _ = tx.send(line.to_string());
        })
        .expect("sh must run");
        assert!(out.status.success());
        let lines: Vec<String> = rx.try_iter().collect();
        assert_eq!(lines, ["a", "b", "c"]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), "a\rb\nc");
    }

    // The whole point of run_timed: a child that outlives its budget is killed
    // and reported as a timeout, promptly — not waited out. `sleep` keeps this
    // portable across Unix; Windows lacks an equivalent stock binary.
    #[cfg(unix)]
    #[test]
    fn run_timed_kills_a_hung_child_promptly() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let started = Instant::now();
        let res = run_timed(cmd, "sleep", Duration::from_millis(300));
        let err = res.expect_err("a 30s sleep must exceed a 300ms budget");
        assert!(err.contains("timed out"), "unexpected error: {err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "child was not killed promptly: {:?}",
            started.elapsed()
        );
    }
}

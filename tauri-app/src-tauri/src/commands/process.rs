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
pub fn hide_console_async(
    cmd: &mut tokio::process::Command,
) -> &mut tokio::process::Command {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// How often the timeout loop wakes to check whether the child has exited.
/// Small enough to react promptly, large enough not to busy-spin.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

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
pub fn run_timed(mut cmd: Command, label: &str, timeout: Duration) -> Result<Output, String> {
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
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
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

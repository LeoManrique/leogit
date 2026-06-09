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

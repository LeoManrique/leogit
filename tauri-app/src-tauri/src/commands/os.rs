//! OS integration for the Changes-tab context menu: reveal a working-tree file
//! in the platform file manager and open it with its default application.
//!
//! Both commands hand the file off to another program (Finder / Explorer /
//! `xdg-open` / the registered handler), so they're `async` (worker thread, no
//! UI block) and bounded by [`process::run_timed`] — a wedged file manager or
//! handler can't hang the app. The launchers are treated as fire-and-forget:
//! once the process exits we report success regardless of its exit code,
//! because some launchers (notably `explorer /select,`) return non-zero even on
//! success. Only a spawn failure (e.g. `xdg-open` not installed) or a timeout
//! surfaces as an error.

use super::process;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Launchers return quickly after handing off; this only catches a hang.
const OS_OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// Spawn a hand-off launcher, bounded by [`process::run_timed`]. A completed
/// run is success regardless of exit code (see module docs).
fn launch(cmd: Command, label: &str) -> Result<(), String> {
    process::run_timed(cmd, label, OS_OPEN_TIMEOUT).map(|_| ())
}

/// Reveal `rel_path` (relative to `repo_path`) in the OS file manager,
/// selecting the item where the platform supports it. On Linux there's no
/// portable "select file", so the containing folder is opened instead.
///
/// # Errors
/// Returns `Err` if the file manager can't be spawned or doesn't return within
/// the timeout.
#[tauri::command(async)]
pub fn reveal_path(repo_path: String, rel_path: String) -> Result<(), String> {
    let abs = PathBuf::from(repo_path).join(rel_path);
    let mut cmd;
    #[cfg(target_os = "macos")]
    {
        cmd = Command::new("open");
        cmd.arg("-R").arg(&abs);
    }
    #[cfg(target_os = "windows")]
    {
        // `/select,<path>` highlights the file in a new Explorer window. Build
        // the arg as an OsString so non-UTF-8 paths survive intact.
        cmd = Command::new("explorer");
        let mut select = std::ffi::OsString::from("/select,");
        select.push(&abs);
        cmd.arg(select);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        cmd = Command::new("xdg-open");
        cmd.arg(abs.parent().unwrap_or(&abs));
    }
    process::hide_console(&mut cmd);
    launch(cmd, "reveal in file manager")
}

/// Open `rel_path` (relative to `repo_path`) with the OS's default application
/// for that file type.
///
/// # Errors
/// Returns `Err` if the handler can't be spawned or doesn't return within the
/// timeout.
#[tauri::command(async)]
pub fn open_path(repo_path: String, rel_path: String) -> Result<(), String> {
    let abs = PathBuf::from(repo_path).join(rel_path);
    let mut cmd;
    #[cfg(target_os = "macos")]
    {
        cmd = Command::new("open");
        cmd.arg(&abs);
    }
    #[cfg(target_os = "windows")]
    {
        // `start` is a `cmd` builtin; the empty "" is its window-title argument
        // so a quoted path isn't mistaken for the title.
        cmd = Command::new("cmd");
        cmd.arg("/c").arg("start").arg("").arg(&abs);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        cmd = Command::new("xdg-open");
        cmd.arg(&abs);
    }
    process::hide_console(&mut cmd);
    launch(cmd, "open with default program")
}

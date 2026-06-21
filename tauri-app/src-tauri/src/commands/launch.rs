//! Opening a repository from the command line (`leogit [dir]`).
//!
//! Two paths reach the same outcome — the frontend switching to a given repo:
//!   * Cold start (app not running): `main.rs` resolves the argv path before the
//!     window exists and stashes it via [`set_pending_open_repo`]; the frontend
//!     pulls it on mount via [`take_pending_open_repo`].
//!   * Warm start (app already running): the single-instance plugin forwards the
//!     second invocation's argv to [`handle_second_instance`], which focuses the
//!     window and emits [`OPEN_REPO_EVENT`] for the live frontend to handle.

use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::git::is_git_repo_path;

/// Repo path captured from a cold-start `leogit <dir>` invocation, held until the
/// frontend mounts and claims it. `None` for a bare launch or a non-repo arg.
///
/// A process-global (set once in `main`, before the window exists) mirrors how
/// the terminal/process modules hold their state, and avoids threading Tauri
/// managed state into a command that only ever reads a single startup value.
static PENDING_OPEN_REPO: Mutex<Option<String>> = Mutex::new(None);

/// Tauri event telling the frontend to switch to a repo. Payload: absolute path.
pub const OPEN_REPO_EVENT: &str = "open-repo";

/// Resolve an argv list to an absolute git-repo path, if one was passed.
///
/// Skips argv\[0\] and takes the first non-flag argument as the target directory;
/// relative paths resolve against `cwd`. Returns `None` when no path was given or
/// it isn't a git repo, so a bare `leogit` (or a stray non-repo arg) just
/// launches/focuses the app instead of erroring. The path is canonicalized so it
/// de-dupes against discovered repos (also canonical) and the frontend's
/// same-repo guard matches.
#[must_use]
pub fn resolve_repo_arg(args: &[String], cwd: &Path) -> Option<String> {
    let raw = args.iter().skip(1).find(|a| !a.starts_with('-'))?;
    let candidate = {
        let p = Path::new(raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    };
    let canonical = std::fs::canonicalize(&candidate).ok()?;
    is_git_repo_path(&canonical).then(|| canonical.to_string_lossy().into_owned())
}

/// Record the cold-start repo path for the frontend to claim on mount.
pub fn set_pending_open_repo(path: Option<String>) {
    if let Some(repo) = &path {
        eprintln!("[launch] cold-start repo from CLI: {repo}");
    }
    if let Ok(mut pending) = PENDING_OPEN_REPO.lock() {
        *pending = path;
    }
}

/// Bring the main window to the foreground (un-minimize, show, focus).
fn focus_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Single-instance callback: a second `leogit [dir]` was launched. Focus the
/// running window and, when a valid repo path was given, tell the frontend to
/// switch to it.
pub fn handle_second_instance(app: &AppHandle, argv: &[String], cwd: &str) {
    eprintln!("[launch] second instance: argv={argv:?} cwd={cwd}");
    focus_main_window(app);
    if let Some(repo) = resolve_repo_arg(argv, Path::new(cwd)) {
        eprintln!("[launch] forwarding open-repo to running window: {repo}");
        let _ = app.emit(OPEN_REPO_EVENT, repo);
    }
}

/// Claim the cold-start repo path (see [`set_pending_open_repo`]). Clears it so a
/// frontend reload doesn't re-open the same repo.
#[must_use]
#[tauri::command]
pub fn take_pending_open_repo() -> Option<String> {
    PENDING_OPEN_REPO
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

//! Opening a folder from the command line (`leogit [dir]`).
//!
//! Two paths reach the same outcome — the frontend acting on a [`LaunchTarget`]:
//!   * Cold start (app not running): `main.rs` resolves the argv path before the
//!     window exists and stashes it via [`set_pending_launch_target`]; the
//!     frontend pulls it on mount via [`take_pending_launch_target`].
//!   * Warm start (app already running): the single-instance plugin forwards the
//!     second invocation's argv to [`handle_second_instance`], which focuses the
//!     window and emits [`OPEN_REPO_EVENT`] for the live frontend to handle.
//!
//! A folder that isn't a repository is still a target: it travels with
//! `is_repo: false` so the frontend can offer to `git init` it, instead of the
//! invocation silently doing nothing.

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::git::repo_root;
use crate::commands::paths;

/// Where a `leogit <dir>` invocation points.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchTarget {
    /// Absolute path — the repository root when `is_repo`, otherwise the folder
    /// exactly as resolved from argv.
    pub path: String,
    /// False when the folder exists but isn't inside a git repository. The
    /// frontend prompts to create one there rather than opening it.
    pub is_repo: bool,
}

/// Target captured from a cold-start `leogit <dir>` invocation, held until the
/// frontend mounts and claims it. `None` for a bare launch or an unusable arg.
///
/// A process-global (set once in `main`, before the window exists) mirrors how
/// the terminal/process modules hold their state, and avoids threading Tauri
/// managed state into a command that only ever reads a single startup value.
static PENDING_LAUNCH_TARGET: Mutex<Option<LaunchTarget>> = Mutex::new(None);

/// Tauri event telling the frontend to act on a folder. Payload: [`LaunchTarget`].
pub const OPEN_REPO_EVENT: &str = "open-repo";

/// Resolve an argv list to the folder it points at, if any.
///
/// Skips argv\[0\] and takes the first non-flag argument as the target
/// directory; relative paths resolve against `cwd`. Returns `None` when no path
/// was given, it doesn't exist, or it isn't a directory — so a bare `leogit`
/// (or `leogit some-file.txt`) just launches/focuses the app instead of
/// erroring. An existing directory always resolves to a target: `is_repo`
/// distinguishes "open this repo" from "offer to create one here".
#[must_use]
pub fn resolve_launch_target(args: &[String], cwd: &Path) -> Option<LaunchTarget> {
    let raw = args.iter().skip(1).find(|a| !a.starts_with('-'))?;
    let candidate = {
        let p = Path::new(raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    };
    let canonical = paths::canonicalize(&candidate).ok()?;
    if !canonical.is_dir() {
        return None;
    }
    Some(repo_root(&canonical).map_or_else(
        || LaunchTarget {
            path: canonical.to_string_lossy().into_owned(),
            is_repo: false,
        },
        |root| LaunchTarget {
            path: root,
            is_repo: true,
        },
    ))
}

/// Record the cold-start target for the frontend to claim on mount.
pub fn set_pending_launch_target(target: Option<LaunchTarget>) {
    if let Some(t) = &target {
        eprintln!(
            "[launch] cold-start target from CLI: {} (repo: {})",
            t.path, t.is_repo
        );
    }
    if let Ok(mut pending) = PENDING_LAUNCH_TARGET.lock() {
        *pending = target;
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
/// running window and, when a folder was given, tell the frontend to open it
/// (or to offer creating a repository there).
pub fn handle_second_instance(app: &AppHandle, argv: &[String], cwd: &str) {
    eprintln!("[launch] second instance: argv={argv:?} cwd={cwd}");
    focus_main_window(app);
    if let Some(target) = resolve_launch_target(argv, Path::new(cwd)) {
        eprintln!(
            "[launch] forwarding open-repo to running window: {} (repo: {})",
            target.path, target.is_repo
        );
        let _ = app.emit(OPEN_REPO_EVENT, target);
    }
}

/// Claim the cold-start target (see [`set_pending_launch_target`]). Clears it so
/// a frontend reload doesn't re-open the same folder.
#[must_use]
#[tauri::command]
pub fn take_pending_launch_target() -> Option<LaunchTarget> {
    PENDING_LAUNCH_TARGET
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn argv(arg: &str) -> Vec<String> {
        vec!["leogit".to_string(), arg.to_string()]
    }

    /// The app's own canonicalizer, not `fs::`, so these assertions compare
    /// against the exact form `resolve_launch_target` hands the frontend.
    fn canonical(path: &Path) -> String {
        paths::canonicalize(path)
            .expect("canonicalize")
            .to_string_lossy()
            .into_owned()
    }

    fn git_init(dir: &Path) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(["init", "-q"])
            .status()
            .expect("spawn git")
            .success();
        assert!(ok, "git init failed");
    }

    /// The case this whole flow exists for: `leogit .` in a plain folder must
    /// still produce a target so the frontend can offer to initialise it. It
    /// used to resolve to `None`, and the invocation silently did nothing.
    #[test]
    fn plain_folder_resolves_to_a_non_repo_target() {
        let tmp = tempdir().expect("tempdir");

        let target = resolve_launch_target(&argv("."), tmp.path()).expect("target");

        assert_eq!(target.path, canonical(tmp.path()));
        assert!(!target.is_repo);
    }

    #[test]
    fn repository_resolves_to_a_repo_target() {
        let tmp = tempdir().expect("tempdir");
        git_init(tmp.path());

        let target = resolve_launch_target(&argv("."), tmp.path()).expect("target");

        assert_eq!(target.path, canonical(tmp.path()));
        assert!(target.is_repo);
    }

    /// Running inside a repo's subfolder opens the repo — it must not look like
    /// an uninitialised folder and prompt to nest a repo inside another.
    #[test]
    fn subdirectory_resolves_to_the_repository_root() {
        let tmp = tempdir().expect("tempdir");
        git_init(tmp.path());
        std::fs::create_dir(tmp.path().join("src")).expect("create src");

        let target = resolve_launch_target(&argv("src"), tmp.path()).expect("target");

        assert_eq!(target.path, canonical(tmp.path()));
        assert!(target.is_repo);
    }

    #[test]
    fn absolute_paths_and_flags_are_handled() {
        let tmp = tempdir().expect("tempdir");
        let absolute = tmp.path().to_string_lossy().into_owned();
        let args = vec!["leogit".to_string(), "--some-flag".to_string(), absolute];

        let target = resolve_launch_target(&args, Path::new("/")).expect("target");

        assert_eq!(target.path, canonical(tmp.path()));
    }

    /// A bare `leogit`, a missing path, or a file just launches/focuses the app.
    #[test]
    fn non_directories_resolve_to_nothing() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("notes.txt");
        std::fs::write(&file, "hi").expect("write file");

        assert!(resolve_launch_target(&["leogit".to_string()], tmp.path()).is_none());
        assert!(resolve_launch_target(&argv("does-not-exist"), tmp.path()).is_none());
        assert!(resolve_launch_target(&argv("notes.txt"), tmp.path()).is_none());
    }
}

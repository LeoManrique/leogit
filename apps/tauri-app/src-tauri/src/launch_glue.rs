//! Window-focusing half of the `leogit <dir>` flow.
//!
//! The pure resolution (argv → [`leogit_core::launch::LaunchTarget`]) lives in
//! core; only the parts that touch the Tauri window live here, driven by the
//! single-instance plugin in `main.rs`.

use std::path::Path;

use leogit_core::launch::{OPEN_REPO_EVENT, resolve_launch_target};
use tauri::{AppHandle, Emitter, Manager};

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

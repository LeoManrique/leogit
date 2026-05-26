#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use leogit_lib::commands::*;

// macOS/Linux apps launched from Finder/.desktop inherit a minimal PATH
// (e.g. /usr/bin:/bin:/usr/sbin:/sbin) and miss user-installed binaries like
// `claude`, `gh`, or homebrew tools. Resolve the user's interactive login PATH
// by spawning their shell once at startup, mirroring what VS Code's `fix-path`
// does. No-op on Windows.
#[cfg(not(target_os = "windows"))]
fn fix_path_env() {
    let shell = match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => s,
        _ => return,
    };
    let output = std::process::Command::new(&shell)
        .arg("-ilc")
        .arg("echo -n \"$PATH\"")
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                std::env::set_var("PATH", path);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn fix_path_env() {}

fn main() {
    fix_path_env();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            config::load_state,
            config::save_state,
            git::get_status,
            git::get_head_sha,
            git::get_diff,
            git::get_diff_whitespace_ignored,
            git::get_commit_diff,
            git::get_selected_diff,
            git::get_log,
            git::get_commit_files,
            git::list_branches,
            git::create_branch,
            git::switch_branch,
            git::delete_branch,
            git::delete_remote_branch,
            git::rename_branch,
            git::commit,
            git::undo_last_commit,
            git::has_staged_changes,
            git::format_commit_message,
            git::fetch,
            git::pull,
            git::push,
            git::get_ahead_behind,
            git::get_remote,
            git::merge_branch,
            git::merge_squash,
            git::commit_squash_merge,
            git::merge_abort,
            git::is_merging,
            git::count_commits_to_merge,
            git::discover_repos,
            git::is_git_repo,
            git::get_repo_name,
            diff::parse_diff,
            diff::generate_patch,
            diff::generate_inverse_patch,
            gh::check_auth,
            ai::generate_commit_message,
            ai::check_provider_available,
            terminal::start_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::close_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

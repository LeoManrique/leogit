#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use leogit_core::launch as core_launch;
use leogit_lib::launch_glue;
use leogit_lib::shims::{
    ai, config, diff, gh, git, highlight, launch, os, shell, terminal, update,
};

fn main() {
    // GUI launches inherit a minimal PATH that misses user-installed tools
    // (`claude`, `gh`, homebrew). Repair it before anything else can be
    // reading the environment — see core's `fix_path_env` contract.
    leogit_core::process::fix_path_env();

    // Resolve a cold-start `leogit <dir>` path before the window exists; the
    // frontend claims it on mount via `take_pending_launch_target`. Warm starts
    // go through the single-instance callback below instead.
    let args: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    core_launch::set_pending_launch_target(core_launch::resolve_launch_target(&args, &cwd));

    tauri::Builder::default()
        // single-instance must be registered first (plugins run in registration
        // order). It forwards a second `leogit <dir>` invocation's argv to the
        // running instance instead of spawning a duplicate window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            launch_glue::handle_second_instance(app, &argv, &cwd);
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            config::load_state,
            config::patch_state,
            config::record_recent_repo,
            git::get_status,
            git::get_head_sha,
            git::get_diff,
            git::get_diff_whitespace_ignored,
            git::get_commit_diff,
            git::get_selected_diff,
            git::get_log,
            git::get_commit_files,
            git::get_commit_stats,
            git::list_branches,
            git::create_branch,
            git::switch_branch,
            git::checkout_commit,
            git::delete_branch,
            git::delete_remote_branch,
            git::rename_branch,
            git::commit,
            git::undo_last_commit,
            git::has_staged_changes,
            git::discard_files,
            git::append_to_gitignore,
            git::ignore_paths,
            os::reveal_path,
            os::open_path,
            os::open_url,
            update::check_for_update,
            git::format_commit_message,
            git::repo_sync_status,
            git::fetch,
            git::pull,
            git::push,
            git::get_ahead_behind,
            git::get_remote,
            git::get_repo_identifier,
            git::merge_branch,
            git::merge_squash,
            git::commit_squash_merge,
            git::merge_abort,
            git::is_merging,
            git::count_commits_to_merge,
            git::discover_repos,
            git::is_git_repo,
            git::init_repo,
            git::get_repo_name,
            git::clone_repo,
            git::get_last_commit_timestamp,
            diff::parse_diff,
            diff::generate_patch,
            diff::generate_inverse_patch,
            highlight::highlight_diff,
            gh::check_auth,
            gh::gh_repo_list,
            gh::gh_clone,
            gh::gh_publish_repo,
            ai::generate_commit_message,
            ai::check_provider_available,
            terminal::terminal_pty_info,
            terminal::start_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::close_terminal,
            shell::list_shells,
            git::effective_scan_paths,
            launch::take_pending_launch_target,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

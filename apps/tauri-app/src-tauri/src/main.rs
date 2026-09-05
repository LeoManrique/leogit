#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use leogit_core::launch as core_launch;
use leogit_lib::launch_glue;
use leogit_lib::shims::{
    ai, config, diff, exclusions, gh, git, highlight, launch, os, repos, shell, terminal, update,
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
        // Restores the window's size and position from the previous run and
        // saves them on exit. The native client gets this from AppKit's frame
        // autosave; without it every Tauri launch reopened at the 1280×800 the
        // config declares, on whichever display the OS picked.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        // The embedded terminal's OSC 52 handler: a shell asking for the
        // clipboard is not a click, and WebKit refuses `navigator.clipboard`
        // without a recent one. This writes from the native side, so a `vim`
        // yank lands whenever it happens rather than only just after a keypress.
        .plugin(tauri_plugin_clipboard_manager::init())
        // The `PATH` above almost always came from a cache, because asking the
        // login shell costs ~430 ms and the window would wait for it. Now that
        // the window exists, ask for real on a worker thread: the answer
        // rewrites the cache and, if it disagrees with what we installed,
        // reaches every child spawned from here on. Never the environment —
        // that is `fix_path_env`'s alone, and only before threads exist.
        .setup(|_app| {
            leogit_core::process::spawn_path_reprobe();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::patch_config,
            config::config_bounds,
            config::load_state,
            config::patch_state,
            config::record_recent_repo,
            git::file_status_styles,
            git::get_status,
            git::get_selected_diff,
            git::get_log,
            git::get_commit_detail,
            git::list_branches,
            git::create_branch,
            git::switch_branch,
            git::checkout_commit,
            git::delete_branch,
            git::commit,
            git::undo_last_commit,
            git::classify_discard,
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
            git::get_remote,
            git::get_repo_identifier,
            git::merge_branch,
            git::merge_squash,
            git::commit_squash_merge,
            git::merge_abort,
            git::count_commits_to_merge,
            git::is_git_repo,
            git::init_repo,
            git::clone_repo,
            diff::get_parsed_diff,
            diff::get_parsed_commit_diff,
            diff::copy_diff_text,
            diff::generate_patch,
            diff::generate_inverse_patch,
            highlight::highlight_diff,
            gh::check_auth,
            gh::gh_repo_list,
            gh::gh_clone,
            gh::gh_publish_repo,
            ai::load_ai_config,
            ai::generate_commit_message,
            ai::check_provider_status,
            ai::provider_status_from_failure,
            terminal::terminal_pty_info,
            terminal::start_terminal,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::close_terminal,
            shell::list_shells,
            git::effective_scan_paths,
            repos::known_repos,
            repos::filter_repos,
            repos::derive_clone_target,
            repos::clone_target_path,
            exclusions::reconcile_exclusions,
            launch::take_pending_launch_target,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: String,
    /// Polling interval for `git fetch`, in milliseconds (default 30000 = 30s).
    pub fetch_interval_ms: u32,
    pub ai_provider: String,
    pub ai_model: Option<String>,
    pub ai_api_key: Option<String>,
    pub auto_fetch: bool,
    pub syntax_highlighting: bool,
    // Added fields
    #[serde(default = "default_scan_paths")]
    pub scan_paths: Vec<String>,
    #[serde(default = "default_scan_depth")]
    pub scan_depth: u32,
    #[serde(default)]
    pub side_by_side_diff: bool,
    #[serde(default)]
    pub hide_whitespace: bool,
    #[serde(default = "default_tab_size")]
    pub tab_size: u32,
    #[serde(default = "default_claude_timeout")]
    pub claude_timeout_secs: u32,
    #[serde(default = "default_ollama_url")]
    pub ollama_server_url: String,
    /// Shell id the embedded terminal launches (see `commands::shell`).
    /// `None` — the default — means "best available for this machine", which
    /// is what every existing config file deserializes to. An id whose shell
    /// has since been uninstalled falls back the same way, so this can never
    /// wedge the terminal.
    #[serde(default)]
    pub terminal_shell: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReposState {
    pub last_opened_repo: Option<String>,
    /// Parent folder the user last cloned into. The Clone dialog pre-fills this
    /// so repeated clones land in the same place; falls back to the first
    /// `scan_path` (then `~/Dev`) the first time.
    #[serde(default)]
    pub last_clone_dir: Option<String>,
    /// Sort mode for the repo picker (`"recent"` | `"name"`). Persisted so the
    /// toggle sticks across restarts; `None` falls back to the recency default.
    #[serde(default)]
    pub repo_sort_mode: Option<String>,
    /// Sort mode for the Clone dialog's GitHub repo list (`"recent"` | `"name"`).
    #[serde(default)]
    pub clone_sort_mode: Option<String>,
    /// Repo paths in most-recently-opened-first order. Drives the picker's
    /// tiered background sync (recently used repos get fetched more often).
    /// Owned entirely by `record_recent_repo` (which de-dupes and caps it).
    /// `None` on first run / pre-migration state files.
    #[serde(default)]
    pub recent_repos: Option<Vec<String>>,
}

/// Field-wise patch for [`ReposState`]: `None` leaves a field as it is on
/// disk. No caller ever needs to clear a field back to "unset", so a single
/// `Option` layer is enough. `recent_repos` is deliberately absent — the MRU
/// list has exactly one writer, `record_recent_repo`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReposStatePatch {
    pub last_opened_repo: Option<String>,
    pub last_clone_dir: Option<String>,
    pub repo_sort_mode: Option<String>,
    pub clone_sort_mode: Option<String>,
}

/// Folders repo discovery falls back to when the configured `scan_paths` list
/// is empty. Also `discover_repos`'s fallback, so the default lives once.
pub(crate) fn default_scan_paths() -> Vec<String> {
    vec![
        "~/Dev".to_string(),
        "~/dev".to_string(),
        "~/code".to_string(),
        "~/Code".to_string(),
        "~/Projects".to_string(),
        "~/src".to_string(),
    ]
}

fn default_scan_depth() -> u32 {
    3
}

fn default_tab_size() -> u32 {
    4
}

fn default_claude_timeout() -> u32 {
    120
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            fetch_interval_ms: 30000,
            ai_provider: "claude".to_string(),
            ai_model: None,
            ai_api_key: None,
            auto_fetch: true,
            syntax_highlighting: true,
            scan_paths: default_scan_paths(),
            scan_depth: default_scan_depth(),
            side_by_side_diff: false,
            hide_whitespace: false,
            tab_size: default_tab_size(),
            claude_timeout_secs: default_claude_timeout(),
            ollama_server_url: default_ollama_url(),
            terminal_shell: None,
        }
    }
}

fn config_dir() -> Result<PathBuf, String> {
    if let Some(dirs) = directories::BaseDirs::new() {
        let config_path = dirs.config_dir().join("leogit");
        fs::create_dir_all(&config_path)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
        Ok(config_path)
    } else {
        Err("Could not determine config directory".to_string())
    }
}

fn config_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.toml"))
}

fn state_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("repos-state.json"))
}

pub fn load_config() -> Result<Config, String> {
    let path = config_file()?;
    if !path.exists() {
        // First run: write defaults to disk so users can discover all options
        let cfg = Config::default();
        let content = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        if let Err(e) = fs::write(&path, content) {
            // Non-fatal: still return defaults to caller
            eprintln!("Warning: could not write default config file: {}", e);
        }
        return Ok(cfg);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {}", e))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
}

pub fn save_config(cfg: Config) -> Result<(), String> {
    let path = config_file()?;
    let content =
        toml::to_string_pretty(&cfg).map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write config file: {}", e))
}

/// Cap on `recent_repos` so repos-state.json can't grow without bound.
const MAX_RECENT_REPOS: usize = 50;

/// Serializes every read-modify-write of repos-state.json. Tauri runs
/// commands concurrently, and two interleaved load+save cycles would silently
/// drop the slower writer's fields.
static STATE_LOCK: Mutex<()> = Mutex::new(());

fn read_state() -> Result<ReposState, String> {
    let path = state_file()?;
    if !path.exists() {
        return Ok(ReposState::default());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read state file: {e}"))?;

    let mut state: ReposState =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse state: {e}"))?;
    normalize_repo_paths(&mut state);
    Ok(state)
}

/// Rewrite the stored paths into the form the rest of the app uses today.
///
/// A state file written by an earlier build holds Windows verbatim paths
/// (`\\?\C:\…`) — that's what `fs::canonicalize` returned before
/// [`paths::canonicalize`] existed. Discovery hands out the ordinary form now,
/// so an unconverted `last_opened_repo` matches no discovered repo (the app
/// forgets which repo was open and drops the user in the picker) and
/// `recent_repos` grows a second entry for a folder it already lists.
///
/// Runs on every read rather than as a one-shot migration: it's idempotent, and
/// the next `update_state` write persists the result, so the file heals itself
/// without a schema version to maintain. A no-op on macOS and Linux, where no
/// path can be in the verbatim form to begin with.
fn normalize_repo_paths(state: &mut ReposState) {
    if let Some(path) = &state.last_opened_repo {
        state.last_opened_repo = Some(paths::simplify_str(path));
    }
    if let Some(path) = &state.last_clone_dir {
        state.last_clone_dir = Some(paths::simplify_str(path));
    }
    if let Some(list) = &mut state.recent_repos {
        for path in &mut *list {
            *path = paths::simplify_str(path);
        }
        // Convert first, then de-dupe: an MRU list that spans the change holds
        // the same folder twice, once in each form, and they collapse here.
        let mut seen = HashSet::new();
        list.retain(|path| seen.insert(path.clone()));
    }
}

fn write_state(state: &ReposState) -> Result<(), String> {
    let path = state_file()?;
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed to serialize state: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write state file: {}", e))
}

/// Apply `mutate` to the on-disk state as one atomic read-modify-write and
/// return the resulting state (the authoritative copy callers reseed from).
fn update_state(mutate: impl FnOnce(&mut ReposState)) -> Result<ReposState, String> {
    // A poisoned lock only means another update panicked mid-write; the guard
    // protects no in-memory data, so continuing is safe.
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A corrupt state file self-heals: start from defaults and let the save
    // below rewrite it, rather than wedging every future update on the same
    // parse error. Matches the frontend's historical fallback behaviour.
    let mut state = read_state().unwrap_or_else(|e| {
        eprintln!("[state] could not read repos-state.json, rewriting it: {e}");
        ReposState::default()
    });
    mutate(&mut state);
    write_state(&state)?;
    Ok(state)
}

fn apply_patch(state: &mut ReposState, patch: ReposStatePatch) {
    if let Some(v) = patch.last_opened_repo {
        state.last_opened_repo = Some(v);
    }
    if let Some(v) = patch.last_clone_dir {
        state.last_clone_dir = Some(v);
    }
    if let Some(v) = patch.repo_sort_mode {
        state.repo_sort_mode = Some(v);
    }
    if let Some(v) = patch.clone_sort_mode {
        state.clone_sort_mode = Some(v);
    }
}

/// Move `path` to the front of the MRU list, de-duplicating and capping length.
fn prepend_recent(list: &mut Vec<String>, path: String) {
    list.retain(|p| *p != path);
    list.insert(0, path);
    list.truncate(MAX_RECENT_REPOS);
}

pub fn load_state() -> Result<ReposState, String> {
    read_state()
}

/// Merge the given fields into repos-state.json atomically, so updating one
/// field (a sort mode, `last_opened_repo`, `last_clone_dir`) can never clobber
/// another writer's field, and return the new state.
///
/// # Errors
/// Returns `Err` when the state file can't be written.
pub fn patch_state(patch: ReposStatePatch) -> Result<ReposState, String> {
    update_state(|state| apply_patch(state, patch))
}

/// Mark a repo as just-opened: move it to the front of the persisted MRU list
/// (most recent first) and return the new state, whose `recent_repos` is the
/// authoritative list the frontend reseeds its store from.
///
/// # Errors
/// Returns `Err` when the state file can't be written.
pub fn record_recent_repo(path: String) -> Result<ReposState, String> {
    update_state(|state| {
        let mut list = state.recent_repos.take().unwrap_or_default();
        prepend_recent(&mut list, path);
        state.recent_repos = Some(list);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_recents(recents: &[&str]) -> ReposState {
        ReposState {
            recent_repos: Some(recents.iter().map(ToString::to_string).collect()),
            ..ReposState::default()
        }
    }

    #[test]
    fn prepend_recent_moves_to_front_and_dedupes() {
        let mut list = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        prepend_recent(&mut list, "b".to_string());
        assert_eq!(list, ["b", "a", "c"]);
    }

    #[test]
    fn prepend_recent_caps_the_list() {
        let mut list: Vec<String> = (0..MAX_RECENT_REPOS).map(|i| format!("repo-{i}")).collect();
        prepend_recent(&mut list, "new".to_string());
        assert_eq!(list.len(), MAX_RECENT_REPOS);
        assert_eq!(list[0], "new");
        assert!(!list.contains(&format!("repo-{}", MAX_RECENT_REPOS - 1)));
    }

    /// Already-ordinary paths survive byte-for-byte. This is the whole of the
    /// macOS/Linux behaviour, and the steady state on Windows once a file has
    /// been converted — which is what makes running this on every read safe.
    #[test]
    fn normalizing_leaves_ordinary_paths_untouched() {
        let mut state = state_with_recents(&["/home/leo/dev/a", "/home/leo/dev/b"]);
        state.last_opened_repo = Some("/home/leo/dev/a".to_string());
        state.last_clone_dir = Some("/home/leo/dev".to_string());
        let before = state.clone();

        normalize_repo_paths(&mut state);

        assert_eq!(state.last_opened_repo, before.last_opened_repo);
        assert_eq!(state.last_clone_dir, before.last_clone_dir);
        assert_eq!(state.recent_repos, before.recent_repos);
    }

    /// The MRU list keeps each folder once, at its most recent position.
    #[test]
    fn normalizing_de_dupes_the_recent_list() {
        let mut state = state_with_recents(&["/dev/a", "/dev/b", "/dev/a"]);
        normalize_repo_paths(&mut state);
        assert_eq!(
            state.recent_repos,
            Some(vec!["/dev/a".to_string(), "/dev/b".to_string()])
        );
    }

    /// A state file written before the path change: verbatim paths convert, and
    /// the two forms of the same folder collapse into one entry rather than
    /// leaving the picker listing it twice.
    #[cfg(windows)]
    #[test]
    fn normalizing_converts_stored_verbatim_paths() {
        let mut state = state_with_recents(&[r"\\?\C:\Dev\a", r"C:\Dev\a", r"C:\Dev\b"]);
        state.last_opened_repo = Some(r"\\?\C:\Dev\a".to_string());
        state.last_clone_dir = Some(r"\\?\C:\Dev".to_string());

        normalize_repo_paths(&mut state);

        assert_eq!(state.last_opened_repo.as_deref(), Some(r"C:\Dev\a"));
        assert_eq!(state.last_clone_dir.as_deref(), Some(r"C:\Dev"));
        assert_eq!(
            state.recent_repos,
            Some(vec![r"C:\Dev\a".to_string(), r"C:\Dev\b".to_string()])
        );
    }

    /// A config file carrying a key from a retired feature
    /// (`show_pull_requests` gated the removed Pull Requests tab,
    /// `wrap_long_lines` toggled the removed no-wrap diff mode) still parses:
    /// unknown keys are ignored, never an error, so a settings change can
    /// retire a field without invalidating every file already on disk.
    /// Guards against a future `deny_unknown_fields`. (The five listed
    /// fields are the ones with no serde default; a real pre-existing file
    /// always carries them.)
    #[test]
    fn config_ignores_retired_keys() {
        let toml = r#"
            theme = "dark"
            fetch_interval_ms = 30000
            ai_provider = "claude"
            auto_fetch = true
            syntax_highlighting = true
            show_pull_requests = false
            wrap_long_lines = false
        "#;
        let config: Config = toml::from_str(toml).expect("a config with a retired key still parses");
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn apply_patch_leaves_absent_fields_untouched() {
        let mut state = state_with_recents(&["kept"]);
        state.last_opened_repo = Some("old-repo".to_string());
        state.repo_sort_mode = Some("name".to_string());

        apply_patch(
            &mut state,
            ReposStatePatch {
                last_clone_dir: Some("/tmp/clones".to_string()),
                ..ReposStatePatch::default()
            },
        );

        assert_eq!(state.last_clone_dir.as_deref(), Some("/tmp/clones"));
        assert_eq!(state.last_opened_repo.as_deref(), Some("old-repo"));
        assert_eq!(state.repo_sort_mode.as_deref(), Some("name"));
        assert_eq!(state.recent_repos, Some(vec!["kept".to_string()]));
    }
}

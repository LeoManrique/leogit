use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::paths;

/// The accepted range for a numeric setting, and what an out-of-range or
/// unparseable value becomes.
///
/// Public because a control's `min`/`max` belongs to the same declaration that
/// enforces it — the three settings surfaces each carried their own copy of
/// these numbers, in two different units, and one of them disagreed with its
/// own control's starting value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bounds {
    pub min: u32,
    pub max: u32,
    pub fallback: u32,
}

impl Bounds {
    /// Round `value` into range. Anything outside becomes the nearest bound —
    /// never the fallback, which is reserved for "there was no value".
    #[must_use]
    pub fn clamp(self, value: u32) -> u32 {
        value.clamp(self.min, self.max)
    }
}

/// Auto-fetch interval. The floor is 5 s because anything faster is a fetch
/// storm rather than a setting; the ceiling is one hour.
pub const FETCH_INTERVAL_MS: Bounds = Bounds {
    min: 5_000,
    max: 3_600_000,
    fallback: 30_000,
};

/// How deep repo discovery walks each scan folder.
pub const SCAN_DEPTH: Bounds = Bounds {
    min: 1,
    max: 10,
    fallback: 3,
};

/// Columns a tab renders as in the diff viewer.
pub const TAB_SIZE: Bounds = Bounds {
    min: 1,
    max: 16,
    fallback: 4,
};

/// How long an AI request may run before it is abandoned.
pub const AI_TIMEOUT_SECS: Bounds = Bounds {
    min: 10,
    max: 3_600,
    fallback: 120,
};

/// Every numeric setting's accepted range, in one place a host can read.
///
/// The three settings surfaces each used to carry their own copy of these
/// numbers — two of them in different units, and one disagreeing with its own
/// control's starting value. A control built from this cannot offer a value
/// the writer will then clamp away.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfigBounds {
    pub fetch_interval_ms: Bounds,
    pub scan_depth: Bounds,
    pub tab_size: Bounds,
    pub ai_timeout_secs: Bounds,
}

/// The bounds every writer enforces, for a host building its controls.
#[must_use]
pub fn config_bounds() -> ConfigBounds {
    ConfigBounds {
        fetch_interval_ms: FETCH_INTERVAL_MS,
        scan_depth: SCAN_DEPTH,
        tab_size: TAB_SIZE,
        ai_timeout_secs: AI_TIMEOUT_SECS,
    }
}

/// Claude-specific AI settings.
///
/// Per provider rather than shared: a single `ai_model` meant setting `sonnet`
/// and switching to Ollama produced a failed request against a model that
/// doesn't exist there, so each provider remembers its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Model id passed to `claude --model`. `None` uses the CLI's own default.
    #[serde(default)]
    pub model: Option<String>,
    /// Seconds a generate request may run. Clamped by [`AI_TIMEOUT_SECS`].
    #[serde(default = "default_ai_timeout")]
    pub timeout_secs: u32,
}

/// Ollama-specific AI settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Model tag to generate with. `None` uses core's commit-message default.
    #[serde(default)]
    pub model: Option<String>,
    /// Base URL of the Ollama server.
    #[serde(default = "default_ollama_url")]
    pub server_url: String,
    /// Seconds a generate request may run. Clamped by [`AI_TIMEOUT_SECS`].
    #[serde(default = "default_ai_timeout")]
    pub timeout_secs: u32,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            model: None,
            timeout_secs: AI_TIMEOUT_SECS.fallback,
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            model: None,
            server_url: default_ollama_url(),
            timeout_secs: AI_TIMEOUT_SECS.fallback,
        }
    }
}

/// The user's settings, as they live in `config.toml`.
///
/// Field order matters: `toml` serializes in declaration order and a table
/// swallows every key after it, so the two provider sections must stay last.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: String,
    /// Polling interval for `git fetch`, in milliseconds. Clamped by
    /// [`FETCH_INTERVAL_MS`].
    pub fetch_interval_ms: u32,
    /// Which AI backend Generate uses: `"claude"` or `"ollama"`. Anything else
    /// normalizes to `"claude"`.
    pub ai_provider: String,
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
    /// Shell id the embedded terminal launches (see `commands::shell`).
    /// `None` — the default — means "best available for this machine", which
    /// is what every existing config file deserializes to. An id whose shell
    /// has since been uninstalled falls back the same way, so this can never
    /// wedge the terminal.
    #[serde(default)]
    pub terminal_shell: Option<String>,
    // --- Tables. Nothing scalar may follow these. ---
    #[serde(default)]
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub ollama: OllamaConfig,
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
    SCAN_DEPTH.fallback
}

fn default_tab_size() -> u32 {
    TAB_SIZE.fallback
}

fn default_ai_timeout() -> u32 {
    AI_TIMEOUT_SECS.fallback
}

/// Where Ollama listens out of the box.
pub(crate) fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            fetch_interval_ms: FETCH_INTERVAL_MS.fallback,
            ai_provider: "claude".to_string(),
            auto_fetch: true,
            syntax_highlighting: true,
            scan_paths: default_scan_paths(),
            scan_depth: default_scan_depth(),
            side_by_side_diff: false,
            hide_whitespace: false,
            tab_size: default_tab_size(),
            terminal_shell: None,
            claude: ClaudeConfig::default(),
            ollama: OllamaConfig::default(),
        }
    }
}

/// Trim a user-typed string and treat "nothing left" as absent.
///
/// The rule the whole config follows: an emptied field means "no value", not
/// "the empty value". Without it `Some("")` sailed past every `unwrap_or`, so
/// an emptied model box ran `claude --model ""` and an emptied server URL made
/// Ollama POST to `/api/generate` with no host — and because both clients share
/// one file, one client's emptied field broke Generate in the other.
fn absent_if_blank(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    /// Return this config with every value inside the range it claims.
    ///
    /// Applied on the way *in* and on the way *out*, so a file hand-edited to
    /// `tab_size = 999`, or written by an older build, reads sanely — and so a
    /// value can't be persisted that a later read would have to defend against.
    /// A clamp is deliberately not a reset: an out-of-range number becomes the
    /// nearest bound, keeping the user's intent ("as big as allowed") rather
    /// than silently reverting to the default.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.fetch_interval_ms = FETCH_INTERVAL_MS.clamp(self.fetch_interval_ms);
        self.scan_depth = SCAN_DEPTH.clamp(self.scan_depth);
        self.tab_size = TAB_SIZE.clamp(self.tab_size);

        self.ai_provider = if self.ai_provider.trim() == "ollama" {
            "ollama".to_string()
        } else {
            // Every unrecognized name lands on claude rather than erroring,
            // which is the guard both clients wrote by hand.
            "claude".to_string()
        };
        self.terminal_shell = absent_if_blank(self.terminal_shell);

        self.claude.model = absent_if_blank(self.claude.model);
        self.claude.timeout_secs = AI_TIMEOUT_SECS.clamp(self.claude.timeout_secs);
        self.ollama.model = absent_if_blank(self.ollama.model);
        self.ollama.timeout_secs = AI_TIMEOUT_SECS.clamp(self.ollama.timeout_secs);
        self.ollama.server_url =
            absent_if_blank(Some(self.ollama.server_url)).unwrap_or_else(default_ollama_url);

        // A blank scan path would expand to the whole home directory.
        self.scan_paths = {
            let mut seen = HashSet::new();
            self.scan_paths
                .into_iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty() && seen.insert(p.clone()))
                .collect()
        };

        self
    }
}

/// Field-wise patch for [`Config`]: `None` leaves a field as it is on disk.
///
/// The whole-object write it replaces was a lost-update waiting to happen —
/// two clients share this file, and a save posted the entire object as it
/// looked when a dialog *opened*, silently reverting whatever the other client
/// had written since. A patch only ever names the fields its surface owns.
///
/// Clearing an optional field is expressed by patching it to `""`: the config's
/// standing rule is that a blank string means absent (see
/// [`Config::normalized`]), so `terminal_shell: Some(String::new())` restores
/// "best available" rather than needing a second layer of `Option`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigPatch {
    pub theme: Option<String>,
    pub fetch_interval_ms: Option<u32>,
    pub ai_provider: Option<String>,
    pub auto_fetch: Option<bool>,
    pub syntax_highlighting: Option<bool>,
    pub scan_paths: Option<Vec<String>>,
    pub scan_depth: Option<u32>,
    pub side_by_side_diff: Option<bool>,
    pub hide_whitespace: Option<bool>,
    pub tab_size: Option<u32>,
    pub terminal_shell: Option<String>,
    pub claude_model: Option<String>,
    pub claude_timeout_secs: Option<u32>,
    pub ollama_model: Option<String>,
    pub ollama_server_url: Option<String>,
    pub ollama_timeout_secs: Option<u32>,
}

fn apply_config_patch(cfg: &mut Config, patch: ConfigPatch) {
    if let Some(v) = patch.theme {
        cfg.theme = v;
    }
    if let Some(v) = patch.fetch_interval_ms {
        cfg.fetch_interval_ms = v;
    }
    if let Some(v) = patch.ai_provider {
        cfg.ai_provider = v;
    }
    if let Some(v) = patch.auto_fetch {
        cfg.auto_fetch = v;
    }
    if let Some(v) = patch.syntax_highlighting {
        cfg.syntax_highlighting = v;
    }
    if let Some(v) = patch.scan_paths {
        cfg.scan_paths = v;
    }
    if let Some(v) = patch.scan_depth {
        cfg.scan_depth = v;
    }
    if let Some(v) = patch.side_by_side_diff {
        cfg.side_by_side_diff = v;
    }
    if let Some(v) = patch.hide_whitespace {
        cfg.hide_whitespace = v;
    }
    if let Some(v) = patch.tab_size {
        cfg.tab_size = v;
    }
    if let Some(v) = patch.terminal_shell {
        cfg.terminal_shell = Some(v);
    }
    if let Some(v) = patch.claude_model {
        cfg.claude.model = Some(v);
    }
    if let Some(v) = patch.claude_timeout_secs {
        cfg.claude.timeout_secs = v;
    }
    if let Some(v) = patch.ollama_model {
        cfg.ollama.model = Some(v);
    }
    if let Some(v) = patch.ollama_server_url {
        cfg.ollama.server_url = v;
    }
    if let Some(v) = patch.ollama_timeout_secs {
        cfg.ollama.timeout_secs = v;
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

/// Serializes every read-modify-write of config.toml.
///
/// Two clients share this file and each runs its commands concurrently, so two
/// interleaved load+save cycles would silently drop the slower writer's fields
/// — the same hazard `repos-state.json` has been protected from all along,
/// which the file people actually edit was missing.
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

/// Read the settings, normalized.
///
/// # Errors
/// When the config directory can't be created, or the file exists but can't be
/// read or parsed.
pub fn load_config() -> Result<Config, String> {
    let path = config_file()?;
    if !path.exists() {
        // First run: write defaults to disk so users can discover all options
        let cfg = Config::default();
        let content = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("Failed to serialize default config: {e}"))?;
        if let Err(e) = fs::write(&path, content) {
            // Non-fatal: still return defaults to caller
            eprintln!("Warning: could not write default config file: {e}");
        }
        return Ok(cfg);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {e}"))?;

    toml::from_str::<Config>(&content)
        .map(Config::normalized)
        .map_err(|e| format!("Failed to parse config: {e}"))
}

/// Write `cfg` out, normalized. Callers hold [`CONFIG_LOCK`].
fn write_config(cfg: &Config) -> Result<(), String> {
    let path = config_file()?;
    let content =
        toml::to_string_pretty(cfg).map_err(|e| format!("Failed to serialize config: {e}"))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write config file: {e}"))
}

/// Apply `patch` to the settings on disk and return the result.
///
/// The only writer. Reading, editing and writing happen under one lock, so a
/// patch can never revert a field it doesn't name — which is what a
/// whole-object save did every time the two clients were open at once. The
/// result is normalized, so a value out of range or blank is corrected *before*
/// it lands rather than defended against on every later read; hand the returned
/// config straight back to the form and it corrects itself.
///
/// # Errors
/// When the current file can't be read, or the new one can't be written.
pub fn patch_config(patch: ConfigPatch) -> Result<Config, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|_| "config lock poisoned")?;
    let mut cfg = load_config()?;
    apply_config_patch(&mut cfg, patch);
    let cfg = cfg.normalized();
    write_config(&cfg)?;
    Ok(cfg)
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
    /// always carries them.) `ai_model` / `ai_api_key` / `claude_timeout_secs`
    /// / `ollama_server_url` are here as the keys the per-provider sections
    /// retired: a file written before that restructure must still open.
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
            ai_model = "sonnet"
            ai_api_key = "sk-whatever"
            claude_timeout_secs = 90
            ollama_server_url = "http://localhost:11434"
        "#;
        let config: Config =
            toml::from_str(toml).expect("a config with a retired key still parses");
        assert_eq!(config.theme, "dark");
        assert_eq!(
            config.claude.timeout_secs, AI_TIMEOUT_SECS.fallback,
            "a retired key does not feed its replacement — the section does"
        );
    }

    /// The provider sections must serialize *after* every scalar. TOML gives a
    /// table everything that follows it, so declaring one earlier would quietly
    /// swallow the settings below it — a file that writes cleanly and reads
    /// back wrong.
    #[test]
    fn config_round_trips_through_toml_with_its_tables_last() {
        let mut cfg = Config::default();
        cfg.claude.model = Some("sonnet".to_string());
        cfg.ollama.model = Some("llama3".to_string());
        cfg.tab_size = 8;
        cfg.terminal_shell = Some("zsh".to_string());

        let text = toml::to_string_pretty(&cfg).expect("serialize");
        let back: Config = toml::from_str(&text).expect("deserialize");

        assert_eq!(back.tab_size, 8, "a scalar after the tables would be lost");
        assert_eq!(back.terminal_shell.as_deref(), Some("zsh"));
        assert_eq!(back.claude.model.as_deref(), Some("sonnet"));
        assert_eq!(back.ollama.model.as_deref(), Some("llama3"));
        assert_eq!(back.ollama.server_url, default_ollama_url());
    }

    /// Each provider keeps its own model. Setting a Claude model and switching
    /// to Ollama used to hand Ollama a model it has never heard of, so Generate
    /// failed with nothing on screen explaining why.
    #[test]
    fn each_provider_remembers_its_own_model() {
        let mut cfg = Config::default();
        cfg.claude.model = Some("sonnet".to_string());
        cfg.ollama.model = Some("llama3".to_string());

        cfg.ai_provider = "claude".to_string();
        let claude = super::super::ai::provider_config(&cfg);
        assert_eq!(claude.model.as_deref(), Some("sonnet"));
        assert_eq!(claude.base_url, None, "claude runs a CLI, not a server");

        cfg.ai_provider = "ollama".to_string();
        let ollama = super::super::ai::provider_config(&cfg);
        assert_eq!(ollama.model.as_deref(), Some("llama3"));
        assert_eq!(
            ollama.base_url.as_deref(),
            Some(default_ollama_url().as_str())
        );
    }

    /// An unrecognized provider name lands on claude rather than erroring —
    /// the guard both clients had written by hand, now written once.
    #[test]
    fn an_unknown_provider_name_normalizes_to_claude() {
        let cfg = Config {
            ai_provider: "gpt-9".to_string(),
            ..Config::default()
        }
        .normalized();
        assert_eq!(cfg.ai_provider, "claude");
        assert_eq!(super::super::ai::provider_config(&cfg).provider, "claude");
    }

    /// The poison D-7 named: an emptied text field persists as `""`, which is
    /// not `None`, so every `unwrap_or` downstream sails past it — `--model ""`
    /// and a server URL of nothing. Normalization is where that stops, for
    /// every writer at once rather than per settings form.
    #[test]
    fn blank_text_fields_normalize_to_absent() {
        let cfg = Config {
            terminal_shell: Some("   ".to_string()),
            claude: ClaudeConfig {
                model: Some(String::new()),
                ..ClaudeConfig::default()
            },
            ollama: OllamaConfig {
                model: Some("  ".to_string()),
                server_url: String::new(),
                ..OllamaConfig::default()
            },
            ..Config::default()
        }
        .normalized();

        assert_eq!(cfg.terminal_shell, None, "blank means best-available");
        assert_eq!(cfg.claude.model, None, "blank means the CLI's own default");
        assert_eq!(cfg.ollama.model, None);
        assert_eq!(
            cfg.ollama.server_url,
            default_ollama_url(),
            "a blank server URL falls back rather than POSTing to nowhere"
        );
    }

    /// Numbers land on the nearest bound, not on the default: an out-of-range
    /// value still says which direction the user wanted. The bounds live once,
    /// so the three settings surfaces that each carried a copy — in two
    /// different units — can't disagree again.
    #[test]
    fn numeric_settings_clamp_to_their_bounds() {
        let huge = Config {
            fetch_interval_ms: u32::MAX,
            scan_depth: 99,
            tab_size: 999,
            claude: ClaudeConfig {
                timeout_secs: 100_000,
                ..ClaudeConfig::default()
            },
            ..Config::default()
        }
        .normalized();
        assert_eq!(huge.fetch_interval_ms, FETCH_INTERVAL_MS.max);
        assert_eq!(huge.scan_depth, SCAN_DEPTH.max);
        assert_eq!(huge.tab_size, TAB_SIZE.max);
        assert_eq!(huge.claude.timeout_secs, AI_TIMEOUT_SECS.max);

        let tiny = Config {
            fetch_interval_ms: 0,
            scan_depth: 0,
            tab_size: 0,
            ..Config::default()
        }
        .normalized();
        assert_eq!(tiny.fetch_interval_ms, FETCH_INTERVAL_MS.min);
        assert_eq!(tiny.scan_depth, SCAN_DEPTH.min);
        assert_eq!(tiny.tab_size, TAB_SIZE.min);
    }

    /// A blank scan path would expand to the whole home folder, and a repeated
    /// one walks the same tree twice.
    #[test]
    fn scan_paths_drop_blanks_and_duplicates() {
        let cfg = Config {
            scan_paths: vec![
                " ~/Dev ".to_string(),
                String::new(),
                "~/Dev".to_string(),
                "~/code".to_string(),
            ],
            ..Config::default()
        }
        .normalized();
        assert_eq!(cfg.scan_paths, ["~/Dev", "~/code"]);
    }

    /// A patch touches only what it names. This is the whole point: two
    /// clients share the file, and the whole-object write it replaces reverted
    /// every field the other client had changed since the dialog opened.
    #[test]
    fn a_patch_leaves_every_field_it_does_not_name_alone() {
        let mut cfg = Config {
            theme: "light".to_string(),
            tab_size: 8,
            hide_whitespace: true,
            ..Config::default()
        };
        cfg.claude.model = Some("sonnet".to_string());

        apply_config_patch(
            &mut cfg,
            ConfigPatch {
                ai_provider: Some("ollama".to_string()),
                ..ConfigPatch::default()
            },
        );

        assert_eq!(cfg.ai_provider, "ollama", "the named field changed");
        assert_eq!(cfg.theme, "light");
        assert_eq!(cfg.tab_size, 8);
        assert!(cfg.hide_whitespace);
        assert_eq!(cfg.claude.model.as_deref(), Some("sonnet"));
    }

    /// Clearing an optional field is patching it to `""` — the config's
    /// standing "blank means absent" rule doing double duty, rather than a
    /// second layer of `Option` every host would have to model.
    #[test]
    fn patching_a_field_blank_clears_it() {
        let mut cfg = Config {
            terminal_shell: Some("fish".to_string()),
            ..Config::default()
        };
        apply_config_patch(
            &mut cfg,
            ConfigPatch {
                terminal_shell: Some(String::new()),
                ..ConfigPatch::default()
            },
        );
        assert_eq!(cfg.normalized().terminal_shell, None);
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

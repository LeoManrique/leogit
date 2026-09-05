use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// The settings file, named once so the writer, the reader and the
/// corruption backup can't drift apart.
const CONFIG_FILE_NAME: &str = "config.toml";

/// The window/repo state file. Not settings: nothing here is hand-edited.
const STATE_FILE_NAME: &str = "repos-state.json";

/// Redirects [`config_dir`] while a test runs.
///
/// These functions address the *user's own* settings by construction — there
/// is no path parameter anywhere in the public surface, deliberately, so no
/// host can point them somewhere else — which leaves a test no way to exercise
/// the writers without rewriting the settings of whoever ran it. The seam is a
/// `static` rather than an argument threaded through eight public functions,
/// so the test harness stays out of the API both hosts call.
///
/// [`ConfigHome`] is the only way to set it, because a redirection that
/// outlives the directory it names would send the next writer into the user's
/// real one.
#[cfg(test)]
static TEST_CONFIG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Only one test at a time may own the redirected config directory, since
/// [`TEST_CONFIG_DIR`] is process-wide and `cargo test` runs in threads.
#[cfg(test)]
static DIR_GUARD: Mutex<()> = Mutex::new(());

/// A throwaway config directory, in force for as long as this value lives.
///
/// Holding [`DIR_GUARD`] for the whole test is what makes the redirection safe:
/// without it a second test would point the same static somewhere else mid-run,
/// and the first would start writing into the second's directory — or, once
/// that one is deleted, into the user's real one.
///
/// It lives beside the static it drives rather than inside this module's tests,
/// because every module that writes into the config directory needs it — the
/// login-`PATH` cache in [`crate::process`] among them — and a second guard
/// with its own mutex would defeat the first: two tests would hold two
/// different locks and redirect the same static at once.
#[cfg(test)]
pub(crate) struct ConfigHome {
    dir: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl ConfigHome {
    pub(crate) fn new() -> Self {
        // A poisoned guard means some earlier test panicked while holding it.
        // It protects no data, so the redirection below is still sound.
        let guard = DIR_GUARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("tempdir");
        *TEST_CONFIG_DIR
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dir.path().to_path_buf());
        Self { dir, _guard: guard }
    }

    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }
}

#[cfg(test)]
impl Drop for ConfigHome {
    fn drop(&mut self) {
        // Before `dir` is deleted and before the guard is released, so no
        // window exists in which the static names a directory that is gone.
        *TEST_CONFIG_DIR
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Development override for [`config_dir`], read fresh on every call.
///
/// The measurement harness (`cargo run --example bench`) times the login-`PATH`
/// cache, which means reading and rewriting a file in this directory; pointed
/// at the real one it would overwrite the cache the user's next app launch
/// reads, and report a timing that depends on whether they had launched the app
/// that week. Neither client sets this and nothing documents it to users: it is
/// a harness seam, not a setting.
const CONFIG_DIR_OVERRIDE: &str = "LEOGIT_CONFIG_DIR";

/// The one directory `LeoGit`'s own files live in — settings, window state, and
/// the login-`PATH` cache.
///
/// `pub(crate)` so that every file the app owns is addressed from here rather
/// than from a second copy of this logic elsewhere in the crate. It stays out
/// of the public surface: hosts get named accessors, never a directory they
/// could point somewhere else.
pub(crate) fn config_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        let redirected = TEST_CONFIG_DIR
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(dir) = redirected {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
            return Ok(dir);
        }
    }
    // Checked after the test redirect, never before: a stray variable in the
    // developer's shell must not be able to steer a test run.
    if let Some(dir) = std::env::var_os(CONFIG_DIR_OVERRIDE).filter(|dir| !dir.is_empty()) {
        let dir = PathBuf::from(dir);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config directory: {e}"))?;
        return Ok(dir);
    }
    if let Some(dirs) = directories::BaseDirs::new() {
        let config_path = dirs.config_dir().join("leogit");
        fs::create_dir_all(&config_path)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
        Ok(config_path)
    } else {
        Err("Could not determine config directory".to_string())
    }
}

fn config_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join(CONFIG_FILE_NAME))
}

fn state_file() -> Result<PathBuf, String> {
    Ok(config_dir()?.join(STATE_FILE_NAME))
}

/// Replace `path`'s contents in one step, so no reader ever sees a half-file.
///
/// `fs::write` truncates the real file and then fills it, which leaves a
/// window — small, but the two clients read these files on their own
/// schedules — where a reader gets zero bytes or a prefix. Writing a sibling
/// temp file and renaming it over the target closes that: a rename within one
/// directory is atomic on every platform we ship, so a reader observes either
/// the whole old file or the whole new one.
///
/// `durable` additionally flushes the bytes (and, on unix, the directory entry
/// that names them) to the storage device, which is what survives power loss
/// rather than merely a crash. It is not free — see [`write_state`] for why
/// one of the two files declines it.
///
/// `pub(crate)` because every file this app writes into its own directory wants
/// the same guarantee, and a second writer with its own truncate-then-fill is
/// how a torn read gets back in.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8], durable: bool) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} names no file", path.display()))?
        .to_string_lossy()
        .into_owned();

    // Same directory as the target, because `fs::rename` is only atomic within
    // one filesystem. Pid *and* nanos: the two clients are separate processes
    // and each may have several threads writing, so neither alone is unique.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let temp = dir.join(format!("{name}.tmp-{}-{nanos}", std::process::id()));

    // `create_new` rather than `create`: if two writers ever did pick the same
    // temp name, interleaving into one file is the failure this whole function
    // exists to prevent, so the second one must fail instead.
    //
    // Opened here rather than inside the body below because *this* is the call
    // whose failure must not trigger the cleanup: an `AlreadyExists` means the
    // file belongs to the other writer, and deleting it would pull the target
    // out from under a save that is midway through succeeding. Past this line
    // the temp exists because of this call, so every later failure is ours to
    // tidy up after.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Failed to create {}: {e}", temp.display()))?;

    match fill_then_rename(file, &temp, path, bytes, durable) {
        Ok(()) => {
            // The bytes being on the device doesn't make the *name* pointing at
            // them durable — that lives in the directory, and only unix lets us
            // fsync one. Best-effort: the data is already safe, so a failure
            // here costs at most this save, and never the file.
            #[cfg(unix)]
            if durable && let Ok(handle) = File::open(dir) {
                let _ = handle.sync_all();
            }
            Ok(())
        }
        Err(e) => {
            // Never leave a `.tmp-…` behind. This is a directory the user opens
            // to hand-edit their settings, and a stray temp file there reads as
            // corruption — or gets edited by mistake.
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}

/// The body of [`write_atomically`] from the temp file's creation onwards,
/// split out so its caller can clean up on every failure path without
/// repeating itself — and only for a temp file that call created.
fn fill_then_rename(
    mut file: File,
    temp: &Path,
    target: &Path,
    bytes: &[u8],
    durable: bool,
) -> Result<(), String> {
    file.write_all(bytes)
        .map_err(|e| format!("Failed to write {}: {e}", temp.display()))?;
    if durable {
        file.sync_all()
            .map_err(|e| format!("Failed to flush {}: {e}", temp.display()))?;
    }
    // Closed before the rename: Windows will not replace a file that is still
    // open without share-delete, which std does not request.
    drop(file);
    // The rename gives the target a *new* inode, born at the process's umask
    // rather than wearing the mode the file had — so a `chmod 600 config.toml`
    // would quietly come back as 644 on the next save. Carry the existing
    // permissions across first. Best-effort: losing the mode is worth less than
    // losing the save, and unix-only because Windows has no mode bits here (its
    // ACLs come from the directory the temp is already in).
    #[cfg(unix)]
    if let Ok(existing) = fs::metadata(target) {
        let _ = fs::set_permissions(temp, existing.permissions());
    }
    rename_over(temp, target)
}

/// Put the temp file in the target's place. Succeeds or fails once: a rename
/// within one directory is atomic here, and nothing else can refuse it.
///
/// Two definitions rather than one with a `cfg` inside, so each platform's
/// body is a whole function the compiler checks on its own terms.
#[cfg(not(windows))]
fn rename_over(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target).map_err(|e| format!("Failed to replace {}: {e}", target.display()))
}

/// Put the temp file in the target's place, retrying a sharing violation.
///
/// Windows replaces atomically too (`MoveFileExW` with `REPLACE_EXISTING`) but
/// refuses while another process holds the target open without share-delete —
/// which is precisely our case, since the other client reads these files on
/// its own schedule. Its read is a couple of kilobytes, so a short backoff
/// outlasts it; anything else fails immediately, since retrying a genuinely
/// broken write only delays the error.
///
/// Three refusals count as contention, not two: `ERROR_SHARING_VIOLATION` (32)
/// is the reader holding the file, `ERROR_LOCK_VIOLATION` (33) is a byte-range
/// lock over it — which is what a `File::lock` on the sidecar's *target* looks
/// like to a mover — and `ERROR_USER_MAPPED_FILE` (1224) is a memory-mapped
/// reader, which is how a file this small is often read. All three end when the
/// other process finishes, so all three are worth waiting out.
///
/// The backoff doubles (10/20/40/80/160 ms, ~310 ms in all) rather than growing
/// by a constant: the common case clears on the first retry and pays 10 ms,
/// while a reader that is genuinely slow gets a useful wait instead of five
/// equally hopeless ones.
///
/// **Not compiled on the platforms this project builds on.** Kept deliberately
/// plain — no platform crates, nothing but `std::io` and raw error numbers — so
/// that it stays reviewable by reading, since nothing here can typecheck it.
#[cfg(windows)]
fn rename_over(temp: &Path, target: &Path) -> Result<(), String> {
    /// Replace attempts before the save is reported failed.
    const ATTEMPTS: u32 = 6;

    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match fs::rename(temp, target) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // std maps some of the same refusals to `PermissionDenied`, so
                // the kind counts alongside the three raw numbers.
                let contended = e.kind() == std::io::ErrorKind::PermissionDenied
                    || matches!(e.raw_os_error(), Some(32 | 33 | 1224));
                if !contended {
                    return Err(format!("Failed to replace {}: {e}", target.display()));
                }
                last = e.to_string();
                if attempt < ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(10u64 << (attempt - 1)));
                }
            }
        }
    }
    Err(format!(
        "Failed to replace {} after {ATTEMPTS} attempts: {last}",
        target.display()
    ))
}

/// Hold the cross-process lock for `name` until the returned handle drops.
///
/// The lock lives on a `<file>.lock` sidecar, never on the file being written:
/// an advisory lock belongs to an open file, and the rename in
/// [`write_atomically`] replaces that file with a different one, so a lock
/// taken on the target would be released halfway through the operation it is
/// meant to cover.
///
/// Every outcome degrades to "unlocked" rather than failing the caller,
/// **including waiting too long**. A filesystem with no advisory locks answers
/// `Unsupported` (some network mounts), and a config directory we cannot create
/// a sidecar in is one the write is about to fail on anyway, with a better
/// message. Unlocked is where this code stood before the sidecar existed, so
/// degrading to it loses nothing that was previously guaranteed.
///
/// The wait is bounded because `File::lock` is not: it blocks until the holder
/// releases, and a holder that is suspended (`^Z` in a terminal, a debugger, a
/// laptop asleep mid-write) never does. This is on the awaited repo-switch
/// path, so an unbounded wait there is an app that has stopped responding to
/// the most common interaction it has. Two seconds is far longer than the
/// milliseconds these writes take and far shorter than a user will sit still
/// for; past it, the other client is not merely busy and ordering ourselves
/// against it is no longer worth the wait.
fn lock_across_processes(name: &str) -> Option<File> {
    /// How long to keep asking before giving up and writing unlocked.
    const WAIT: Duration = Duration::from_secs(2);
    /// Gap between attempts. Short enough that the ordinary case — the other
    /// client mid-write — costs one tick, and cheap enough to repeat.
    const RETRY_AFTER: Duration = Duration::from_millis(10);

    let path = config_dir().ok()?.join(format!("{name}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .ok()?;

    // `Instant`, not the wall clock: a clock that steps backwards mid-wait
    // (NTP, a timezone change) would otherwise extend this indefinitely, which
    // is the exact failure the bound exists to prevent.
    let deadline = Instant::now() + WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            // A filesystem that cannot do this at all says so on the first
            // attempt, and will keep saying it: returning immediately keeps a
            // network mount from paying the whole wait on every single write.
            Err(TryLockError::Error(e)) if e.kind() == std::io::ErrorKind::Unsupported => {
                return None;
            }
            Err(TryLockError::Error(e)) => {
                eprintln!("[config] could not take the {name} lock, continuing unlocked: {e}");
                return None;
            }
            Err(TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    eprintln!(
                        "[config] the {name} lock is still held after {}s, continuing unlocked",
                        WAIT.as_secs()
                    );
                    return None;
                }
                std::thread::sleep(RETRY_AFTER);
            }
        }
    }
}

/// Serializes every read and every read-modify-write of config.toml *within
/// this process*.
///
/// Two clients share this file and each runs its commands concurrently, so two
/// interleaved load+save cycles would silently drop the slower writer's
/// fields. A process-local mutex cannot see the other client at all, which is
/// why [`load_config`] and [`patch_config`] take both this and the
/// `config.toml.lock` sidecar: this one orders the threads, the sidecar orders
/// the processes.
///
/// The plain read holds it too because it is not a plain read — it heals a
/// corrupt file, which writes. See [`load_config`].
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

/// Read the settings, normalized.
///
/// Takes both locks, because reading this file is not only a read: a file that
/// will not parse is *healed*, which moves the user's bytes aside and writes
/// defaults over the name. Two clients doing that at once — both launching, or
/// both polling settings — would each keep a backup, and the second would move
/// the first's freshly written defaults on top of the first's backup of the
/// user's text. The bytes the whole heal exists to preserve would then be gone,
/// with two files left behind that both say `theme = "dark"`. Under the locks
/// the loser reads what the winner wrote and heals nothing.
///
/// # Errors
/// When the config directory can't be created, or the file exists but can't be
/// read.
pub fn load_config() -> Result<Config, String> {
    // A poisoned lock only means some other read or patch panicked; the guard
    // protects no in-memory data, so continuing is safe — and refusing would
    // leave the app unable to read its own settings for the rest of the
    // session. `STATE_LOCK` recovers for the same reason.
    let _guard = CONFIG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _shared = lock_across_processes(CONFIG_FILE_NAME);
    load_config_locked()
}

/// [`load_config`]'s body, for callers that already hold both locks.
///
/// Separate rather than re-entrant: `CONFIG_LOCK` is a `std::sync::Mutex`,
/// which deadlocks a thread that locks it twice, and a second `File::lock` on
/// the sidecar would block on a lock this very thread is holding through
/// another descriptor — a wait nothing can ever end.
fn load_config_locked() -> Result<Config, String> {
    let path = config_file()?;
    if !path.exists() {
        // First run: write defaults to disk so users can discover all options
        let cfg = Config::default();
        if let Err(e) = write_config(&cfg) {
            // Non-fatal: still return defaults to caller
            eprintln!("Warning: could not write default config file: {e}");
        }
        return Ok(cfg);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {e}"))?;

    match toml::from_str::<Config>(&content) {
        Ok(cfg) => Ok(cfg.normalized()),
        // A file we *could* read and could not understand heals; a file we
        // could not read at all still errors above, because that is a
        // permissions or hardware problem and overwriting it would destroy
        // settings that are perfectly intact.
        Err(e) => Ok(heal_corrupt_config(&path, &e.to_string())),
    }
}

/// Keep a `config.toml` that will not parse, and start over from defaults.
///
/// `repos-state.json` has healed itself this way all along (see
/// [`update_state`]) while this file — the one the user actually hand-edits,
/// and so the one that can acquire a stray character — hard-errored instead.
/// Because [`patch_config`] reads before it writes, that made a single typo
/// unrepairable from Settings: every save failed on the same parse error until
/// the file was deleted by hand.
///
/// The bytes are moved aside rather than dropped. They are the user's own
/// text, and a backup they can open is what makes "your settings were reset"
/// recoverable instead of merely true.
///
/// Only ever called with both of [`load_config`]'s locks held, which is what
/// keeps one heal from overwriting another's backup. The name is unique anyway
/// — seconds, pid and nanos — so that a second heal in the same second, or one
/// from a process that could not take the lock at all, still lands beside the
/// first rather than on top of it. A backup that can be silently replaced is
/// not a backup.
fn heal_corrupt_config(path: &Path, reason: &str) -> Config {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    let secs = now.as_ref().map_or(0, Duration::as_secs);
    let nanos = now.as_ref().map_or(0, Duration::subsec_nanos);
    // Built from the path we were handed rather than from the constant: they
    // are the same file today, and a backup named after a different one than
    // the one that was moved is the kind of drift a shared constant is supposed
    // to prevent, not cause.
    let name = path.file_name().unwrap_or(CONFIG_FILE_NAME.as_ref());
    let backup = path.with_file_name(format!(
        "{}.corrupt-{secs}-{}-{nanos}",
        name.to_string_lossy(),
        std::process::id()
    ));
    match fs::rename(path, &backup) {
        Ok(()) => eprintln!(
            "[config] {CONFIG_FILE_NAME} did not parse ({reason}); kept as {} and reset to defaults",
            backup.display()
        ),
        Err(e) => eprintln!(
            "[config] {CONFIG_FILE_NAME} did not parse ({reason}) and could not be kept aside ({e}); resetting to defaults"
        ),
    }
    let cfg = Config::default();
    if let Err(e) = write_config(&cfg) {
        eprintln!("[config] could not write a default {CONFIG_FILE_NAME}: {e}");
    }
    cfg
}

/// Write `cfg` out exactly as given.
///
/// It does **not** normalize: [`patch_config`] normalizes before it calls here,
/// so that the value it returns to the caller is the value on disk, and the
/// other two callers write [`Config::default`], which is normal by
/// construction. Normalizing here as well would make the returned config and
/// the file agree only by coincidence.
///
/// Durable, unlike [`write_state`]: this file is written only when the user
/// saves Settings, so the `F_FULLFSYNC` it costs on macOS is paid at most once
/// per deliberate action — and it holds text the user typed, which is the kind
/// of loss they would notice and could not reconstruct.
///
/// Takes no lock of its own, and must not: every path that reaches it —
/// [`patch_config`], and [`load_config_locked`]'s first-run and self-heal
/// branches — is already holding both, and `CONFIG_LOCK` deadlocks on a second
/// lock from the same thread.
fn write_config(cfg: &Config) -> Result<(), String> {
    let path = config_file()?;
    let content =
        toml::to_string_pretty(cfg).map_err(|e| format!("Failed to serialize config: {e}"))?;
    write_atomically(&path, content.as_bytes(), true)
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
    // Poisoning is recovered from rather than reported: see [`load_config`].
    let _guard = CONFIG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Taken *inside* the process-local lock, so only one thread here can ever
    // be waiting on the other client — an advisory lock is per open file, and
    // two threads of ours contending on it would deadlock nothing but would
    // make the ordering harder to reason about than it needs to be.
    let _shared = lock_across_processes(CONFIG_FILE_NAME);
    // The `_locked` body, never the public one: both locks are already in this
    // thread's hands and neither is re-entrant.
    let mut cfg = load_config_locked()?;
    apply_config_patch(&mut cfg, patch);
    let cfg = cfg.normalized();
    write_config(&cfg)?;
    Ok(cfg)
}

/// Cap on `recent_repos` so repos-state.json can't grow without bound.
const MAX_RECENT_REPOS: usize = 50;

/// Serializes every read-modify-write of repos-state.json *within this
/// process*. Tauri runs commands concurrently, and two interleaved load+save
/// cycles would silently drop the slower writer's fields. [`update_state`]
/// pairs it with the `repos-state.json.lock` sidecar, which is what orders the
/// two clients against each other.
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

/// Write the state out atomically, but **not** durably.
///
/// The hazard this file has is a torn *read*: both clients poll it, and a
/// truncate-then-fill write let one of them see an empty or half-written file
/// and forget which repo was open. The rename closes that.
///
/// Power-loss durability is deliberately not bought on top. `sync_all` is
/// `F_FULLFSYNC` on macOS — tens of milliseconds, since it waits on the drive
/// itself — and this file is rewritten on every repo switch, with the Tauri
/// client awaiting that write before it continues. Paying that on the switch
/// path would put a visible stall into the most common interaction in the app,
/// to protect a preferences file whose worst-case loss is one entry of an MRU
/// list. [`write_config`] makes the opposite trade, for the opposite reasons.
fn write_state(state: &ReposState) -> Result<(), String> {
    let path = state_file()?;
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed to serialize state: {}", e))?;

    write_atomically(&path, content.as_bytes(), false)
}

/// Apply `mutate` to the on-disk state as one atomic read-modify-write and
/// return the resulting state (the authoritative copy callers reseed from).
fn update_state(mutate: impl FnOnce(&mut ReposState)) -> Result<ReposState, String> {
    // A poisoned lock only means another update panicked mid-write; the guard
    // protects no in-memory data, so continuing is safe.
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The other half of the guarantee: `STATE_LOCK` orders our own threads,
    // this orders us against the other client, which shares the file.
    let _shared = lock_across_processes(STATE_FILE_NAME);
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
/// (most recent first), record it as the repo to restore on the next launch,
/// and return the new state — whose `recent_repos` is the authoritative list
/// the frontend reseeds its store from.
///
/// The two facts are written together because they are one fact. Both clients
/// used to make this call and then immediately patch `last_opened_repo`
/// separately, which is two full read-parse-serialize-write cycles of the same
/// file per repo switch — and two windows in which a reader could catch the
/// file between them, listing a repo as most-recent that the app would not
/// reopen. Every caller of this is an open, so there is no caller that wants
/// the recency without the restore point.
///
/// Each client calls it **once per open**, and both arrange for that
/// structurally rather than by remembering to: Tauri writes from the view that
/// a newly opened repo mounts (and from its one in-app switch), and the native
/// client from the screen's per-repository task. A second call would not
/// corrupt anything — it is idempotent — but it would be a second full
/// read-modify-write of this file on the path the user waits on.
///
/// # Errors
/// Returns `Err` when the state file can't be written.
pub fn record_recent_repo(path: String) -> Result<ReposState, String> {
    update_state(|state| {
        let mut list = state.recent_repos.take().unwrap_or_default();
        prepend_recent(&mut list, path.clone());
        state.recent_repos = Some(list);
        state.last_opened_repo = Some(path);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ConfigHome {
        /// Every entry whose name starts with `prefix`, in sorted order.
        fn entries_starting_with(&self, prefix: &str) -> Vec<PathBuf> {
            let mut names: Vec<PathBuf> = fs::read_dir(self.path())
                .expect("read the config dir")
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
                })
                .collect();
            names.sort();
            names
        }

        /// The one entry whose name starts with `prefix`, or `None` when there
        /// is none.
        ///
        /// A second match is a failure, not a choice to make: every caller
        /// means "the file this write left behind", and picking one of two
        /// would hide exactly the bug — a stray temp file, a backup landing on
        /// another backup — these assertions exist to catch. So it panics,
        /// naming everything that *is* there.
        fn entry_starting_with(&self, prefix: &str) -> Option<PathBuf> {
            let names = self.entries_starting_with(prefix);
            assert!(
                names.len() <= 1,
                "expected at most one {prefix}* in the config dir, found {names:?}"
            );
            names.into_iter().next()
        }
    }

    /// The whole point of the temp-file-and-rename writer: the new bytes
    /// replace the old ones, and nothing is left in the directory besides the
    /// file itself. A stray `.tmp-…` in the folder the user opens to edit
    /// their settings reads as corruption.
    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp() {
        let home = ConfigHome::new();
        let target = home.path().join("thing.txt");

        write_atomically(&target, b"first", false).expect("first write");
        write_atomically(&target, b"second", true).expect("durable overwrite");

        assert_eq!(fs::read_to_string(&target).expect("read back"), "second");
        assert_eq!(
            home.entry_starting_with("thing.txt.tmp-"),
            None,
            "the temp file must not outlive the write"
        );
    }

    /// The cleanup path. A rename that cannot land — here because the target
    /// is a directory — must still take its temp file with it, or a failed
    /// save leaves litter behind every time it fails.
    #[test]
    fn atomic_write_cleans_up_after_a_failed_replace() {
        let home = ConfigHome::new();

        let occupied = home.path().join("occupied");
        fs::create_dir(&occupied).expect("create the blocking directory");
        assert!(
            write_atomically(&occupied, b"bytes", false).is_err(),
            "renaming a file over a directory cannot succeed"
        );
        assert_eq!(
            home.entry_starting_with("occupied.tmp-"),
            None,
            "a failed replace must remove its own temp file"
        );

        assert!(
            write_atomically(&home.path().join("missing/deep.txt"), b"bytes", false).is_err(),
            "a target whose directory does not exist is an error, not a panic"
        );
    }

    /// A `config.toml` with a typo in it used to be unrepairable from Settings:
    /// `patch_config` reads before it writes, so every save failed on the same
    /// parse error until the file was deleted by hand. It now heals the way
    /// `repos-state.json` always has — keeping the user's bytes beside it,
    /// because they are the only copy of what they typed.
    #[test]
    fn a_corrupt_config_heals_and_keeps_the_original_bytes() {
        let home = ConfigHome::new();
        let original = "theme = \"dark\"\nthis line is not = = toml\n";
        fs::write(home.path().join(CONFIG_FILE_NAME), original).expect("seed a corrupt config");

        let healed = load_config().expect("a config that will not parse must not be fatal");
        assert_eq!(healed.theme, Config::default().theme);

        let backup = home
            .entry_starting_with(&format!("{CONFIG_FILE_NAME}.corrupt-"))
            .expect("the unparseable bytes are kept beside the reset file");
        assert_eq!(
            fs::read_to_string(&backup).expect("read the backup"),
            original,
            "the backup is the user's text, byte for byte"
        );

        // The point of healing: Settings can save again immediately.
        let patched = patch_config(ConfigPatch {
            theme: Some("light".to_string()),
            ..ConfigPatch::default()
        })
        .expect("a patch after the heal");
        assert_eq!(patched.theme, "light");
        assert_eq!(load_config().expect("reload").theme, "light");
    }

    /// A backup that another heal can overwrite is not a backup. Two heals in
    /// the same second — two clients launching together, or one user who
    /// mistypes the file twice in a minute — used to share a name, so the
    /// second `fs::rename` moved a file of defaults on top of the only copy of
    /// what the user had written.
    #[test]
    fn a_second_heal_keeps_its_own_backup() {
        let home = ConfigHome::new();
        let path = home.path().join(CONFIG_FILE_NAME);
        let texts = ["first = = broken\n", "second = = broken\n"];

        for text in texts {
            fs::write(&path, text).expect("seed a corrupt config");
            load_config().expect("a config that will not parse must not be fatal");
        }

        let backups = home.entries_starting_with(&format!("{CONFIG_FILE_NAME}.corrupt-"));
        assert_eq!(backups.len(), 2, "one backup per heal: {backups:?}");
        let kept: HashSet<String> = backups
            .iter()
            .map(|path| fs::read_to_string(path).expect("read a backup"))
            .collect();
        assert_eq!(
            kept,
            texts.iter().map(ToString::to_string).collect(),
            "each heal kept its own user's text, not a copy of the defaults"
        );
    }

    /// The wait for the other client ends. `File::lock` blocks until the holder
    /// releases it, and a holder that is suspended — `^Z`, a debugger, a laptop
    /// asleep mid-write — never does; this call sits on the awaited repo-switch
    /// path, so "waits forever" there reads as an app that has hung.
    ///
    /// The sidecar is held through a second open of the same file, which is a
    /// second file description and so contends exactly as another process
    /// would.
    #[test]
    fn a_held_lock_is_waited_for_and_then_given_up_on() {
        let home = ConfigHome::new();
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(home.path().join(format!("{CONFIG_FILE_NAME}.lock")))
            .expect("open the sidecar");
        held.lock().expect("hold it the way the other client would");

        let started = Instant::now();
        let taken = lock_across_processes(CONFIG_FILE_NAME);
        let waited = started.elapsed();

        assert!(
            taken.is_none(),
            "a lock the other client is holding cannot also be ours"
        );
        assert!(
            waited >= Duration::from_secs(1),
            "it must wait for the other client rather than give up at once: {waited:?}"
        );
        assert!(
            waited < Duration::from_secs(10),
            "and it must give up rather than block for as long as the holder lives: {waited:?}"
        );
    }

    /// A reader with no lock at all — which is what `load_state` is, and what
    /// the other client's poll is — never catches the file between the old
    /// bytes and the new ones. Two payloads of different content but the same
    /// size, so a torn read cannot pass by looking like the other one.
    ///
    /// A smoke test, and honest about it: it can only catch the regression
    /// probabilistically. Restore `fs::write` under this and it fails within a
    /// few hundred alternations on every machine tried, but it fails because
    /// the truncate window is wide, not because anything here forces the reader
    /// into it. It is the failing direction that matters — a pass is weak
    /// evidence, a failure is proof.
    #[test]
    fn an_atomic_write_is_never_read_half_finished() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        /// Big enough that a truncate-then-fill write spans several
        /// filesystem operations, which is what opens the window.
        const PAYLOAD: usize = 4096;
        const ALTERNATIONS: usize = 300;

        let home = ConfigHome::new();
        let target = home.path().join("racing.txt");
        let payloads = ["a".repeat(PAYLOAD), "b".repeat(PAYLOAD)];
        write_atomically(&target, payloads[0].as_bytes(), false).expect("seed the file");

        let stop = AtomicBool::new(false);
        let reads = AtomicUsize::new(0);
        let empty = AtomicUsize::new(0);
        let torn = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let reader = scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    // A failed read is the rename's own window on some
                    // platforms and is not what this measures; only what the
                    // reader actually *got* is judged.
                    let Ok(text) = fs::read_to_string(&target) else {
                        continue;
                    };
                    reads.fetch_add(1, Ordering::Relaxed);
                    if text.is_empty() {
                        empty.fetch_add(1, Ordering::Relaxed);
                    } else if !payloads.contains(&text) {
                        torn.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
            for i in 0..ALTERNATIONS {
                write_atomically(&target, payloads[i % 2].as_bytes(), false).expect("write");
            }
            stop.store(true, Ordering::Relaxed);
            reader.join().expect("the reader thread");
        });

        assert!(
            reads.load(Ordering::Relaxed) > 0,
            "the reader never managed a read, so it proved nothing"
        );
        assert_eq!(
            empty.load(Ordering::Relaxed),
            0,
            "an empty read is the truncate window this writer exists to close"
        );
        assert_eq!(
            torn.load(Ordering::Relaxed),
            0,
            "a read that matched neither payload saw a partially written file"
        );
    }

    /// Two writers hammering the state file while a third reads it without any
    /// lock at all — which is exactly the shape of the two clients, since
    /// `load_state` takes none.
    ///
    /// The load-bearing assertion is the reader's: an unlocked read never
    /// observes a file that will not parse. The two writers are here to give it
    /// something to race, and to exercise `update_state` under contention.
    ///
    /// **The `recents.len()` assertion proves nothing about atomicity**, and
    /// the comment that said it did was wrong: `STATE_LOCK` serialises these
    /// two threads before either touches the disk, so the count would hold even
    /// with the old truncating writer. It is kept because it is still the
    /// cheapest check that `update_state`'s read-modify-write does not drop an
    /// update under load — a lost-update guard, not a torn-write one. The
    /// writer's own atomicity is covered by
    /// `an_atomic_write_is_never_read_half_finished`.
    #[test]
    fn concurrent_state_updates_never_tear_the_file() {
        const PER_THREAD: usize = 50;

        let _home = ConfigHome::new();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let unparseable = std::sync::atomic::AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let writers = ["a", "b"].map(|tag| {
                scope.spawn(move || {
                    for i in 0..PER_THREAD {
                        update_state(|state| {
                            let mut list = state.recent_repos.take().unwrap_or_default();
                            list.push(format!("{tag}-{i}"));
                            state.recent_repos = Some(list);
                        })
                        .expect("update the state");
                    }
                })
            });
            let reader = scope.spawn(|| {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if load_state().is_err() {
                        unparseable.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    std::thread::yield_now();
                }
            });
            for writer in writers {
                writer.join().expect("a writer thread");
            }
            // Only once the writers are done, so the reader has been racing
            // real writes for the whole run rather than an idle file.
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            reader.join().expect("the reader thread");
        });

        let final_state = load_state().expect("the state file still parses");
        let recents = final_state.recent_repos.unwrap_or_default();
        assert_eq!(
            recents.len(),
            PER_THREAD * 2,
            "every update must survive: a lost one means a read saw a torn file \
             and started over from defaults"
        );
        for tag in ["a", "b"] {
            let seen = recents.iter().filter(|p| p.starts_with(tag)).count();
            assert_eq!(seen, PER_THREAD, "all of {tag}'s updates landed");
        }
        assert_eq!(
            unparseable.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "an unlocked reader must never observe a partially written file"
        );
    }

    /// Opening a repo is one fact, so it is one write. Both clients used to
    /// call this and then patch `last_opened_repo` separately — two full
    /// rewrites of the file per switch, and a window between them where the
    /// most-recent repo and the one that would reopen disagreed.
    #[test]
    fn recording_a_recent_repo_also_sets_the_restore_point() {
        let _home = ConfigHome::new();

        let first = record_recent_repo("/dev/one".to_string()).expect("record");
        assert_eq!(first.last_opened_repo.as_deref(), Some("/dev/one"));

        let second = record_recent_repo("/dev/two".to_string()).expect("record");
        assert_eq!(second.last_opened_repo.as_deref(), Some("/dev/two"));
        assert_eq!(
            second.recent_repos,
            Some(vec!["/dev/two".to_string(), "/dev/one".to_string()]),
            "the MRU still leads with the newest"
        );
        assert_eq!(
            load_state().expect("reload").last_opened_repo.as_deref(),
            Some("/dev/two"),
            "and it reached disk, which is what the next launch reads"
        );
    }

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

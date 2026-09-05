//! The on-disk answer to "is the login `PATH` we already probed still true?"
//!
//! Asking the user's login shell what `PATH` their terminal would have costs
//! ~430 ms, and both hosts must have the answer before they start anything else
//! (see [`crate::process::fix_path_env`] for why that ordering is a soundness
//! requirement rather than a preference). That is the single largest number on
//! the startup path, and it is paid on every launch for a value that changes
//! perhaps twice a year.
//!
//! So the probe's result is cached in the config directory, and this module
//! owns the only interesting question that raises: **when is the cached value
//! wrong?** Two independent answers, because neither alone is enough:
//!
//! 1. **A structural key.** Every file that could contribute to the login
//!    `PATH` — the shell's rc files, `/etc/paths` and `/etc/paths.d`, the
//!    version managers' state files — is recorded with its mtime and size,
//!    *whether or not it exists*. Absence is a recorded fact, so creating a
//!    `.zprofile` that was never there invalidates the cache just as editing
//!    one does.
//! 2. **A ceiling on age.** The key cannot see everything: `nvm install 22`
//!    rewrites a directory the key does not name, and moving the Homebrew
//!    prefix changes what an unchanged `.zprofile` *evaluates to*. A cache
//!    older than [`MAX_AGE`] is re-probed regardless of what the key says.
//!
//! The cache lives with the settings rather than in a cache directory on
//! purpose: a wiped cache directory is supposed to cost nothing but time, and
//! this file costs the user a visibly slower launch. Losing it is still only
//! that — one slow launch — which is why the write is not made durable.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::config;

/// The cache file, alongside `config.toml` and `repos-state.json`.
pub(crate) const CACHE_FILE_NAME: &str = "shell-path-cache.json";

/// Bumped whenever the *meaning* of a field changes — a different key
/// composition, say. An older file is then discarded rather than misread, which
/// is the whole reason the number is written down instead of inferred.
const CACHE_VERSION: u32 = 1;

/// How long a cache entry is trusted even when nothing in its key moved.
///
/// Seven days is a compromise between the two failure modes: shorter and a user
/// who launches the app weekly pays the probe every time; longer and a new
/// toolchain installed by a manager the key cannot see stays invisible for a
/// month. The background re-probe means the ceiling is only ever hit by a
/// launch, never by a running app.
const MAX_AGE: Duration = Duration::from_hours(7 * 24);

/// One recorded file: its path, its mtime in nanoseconds since the epoch, and
/// its size. Both numbers are `None` for a file that does not exist — which is
/// a value the comparison treats like any other, so a file *appearing* is as
/// much a change as a file being edited.
///
/// A tuple rather than a struct because this is the file's wire shape as well
/// as its in-memory one, and `["/etc/zshrc", 1757…, 1042]` is a readable line
/// in a file a developer may well open.
type KeyEntry = (String, Option<u128>, Option<u64>);

/// Environment variables that decide *which* files the key above should name.
/// Folded into the key as pseudo-entries so that changing one — a new `SHELL`,
/// a `ZDOTDIR` that redirects zsh's rc files elsewhere — invalidates the cache
/// through the same comparison that catches an edited file.
const KEY_VARS: &[&str] = &["SHELL", "HOME", "ZDOTDIR", "XDG_CONFIG_HOME"];

/// The cached probe result, exactly as it is stored.
#[derive(Serialize, Deserialize)]
struct PathCache {
    version: u32,
    shell: String,
    path: String,
    key: Vec<KeyEntry>,
    probed_at: u64,
}

/// A cache entry that survived every check, with how old it turned out to be —
/// the caller logs the age, because "cached 6 days ago" is the line that
/// explains a `PATH` a user disagrees with.
#[derive(Debug)]
pub(crate) struct Hit {
    pub(crate) path: String,
    pub(crate) age: Duration,
}

/// The cached login `PATH` for `shell`, or the first reason it was rejected.
///
/// The reason is a sentence, not a code, because its only consumer is a log
/// line the user or a bug report will read.
///
/// # Errors
/// Every rejection is an `Err`, including "there is no cache yet" — a first
/// launch and a stale cache take the same path through the caller.
pub(crate) fn load(shell: &str) -> Result<Hit, String> {
    let cache = read()?;
    let age = validate(&cache, shell, &path_cache_key(shell), now_secs())?;
    Ok(Hit {
        path: cache.path,
        age,
    })
}

/// Record `path` as the login `PATH` `shell` printed, keyed on the current
/// state of every file that could have contributed to it.
///
/// # Errors
/// Returns `Err` when the config directory or the file itself can't be written.
pub(crate) fn store(shell: &str, path: &str) -> Result<(), String> {
    let cache = PathCache {
        version: CACHE_VERSION,
        shell: shell.to_string(),
        path: path.to_string(),
        key: path_cache_key(shell),
        probed_at: now_secs(),
    };
    let bytes =
        serde_json::to_vec_pretty(&cache).map_err(|e| format!("could not serialize cache: {e}"))?;
    // Atomic, but deliberately not durable: the other client may read this file
    // at any moment, so a torn read has to be impossible — while losing the
    // whole file to a power cut costs exactly one slow launch, which is not
    // worth an `fsync` on a path whose entire purpose is to be fast.
    config::write_atomically(&cache_file()?, &bytes, false)
}

fn cache_file() -> Result<PathBuf, String> {
    Ok(config::config_dir()?.join(CACHE_FILE_NAME))
}

fn read() -> Result<PathCache, String> {
    let file = cache_file()?;
    let bytes = fs::read(&file).map_err(|e| format!("no usable cache ({e})"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("cache file is unreadable ({e})"))
}

/// Decide whether `cache` still describes `shell`'s login `PATH`, given the key
/// built from the filesystem right now and the current time.
///
/// Pure, and takes both of the things that would otherwise be read from the
/// world, so every rejection rule is testable without a machine in the right
/// state. The order of the checks is the order of the answers' cost: two field
/// comparisons, then the key, then arithmetic.
fn validate(
    cache: &PathCache,
    shell: &str,
    key: &[KeyEntry],
    now: u64,
) -> Result<Duration, String> {
    if cache.version != CACHE_VERSION {
        return Err(format!(
            "cache is version {}, this build writes {CACHE_VERSION}",
            cache.version
        ));
    }
    if cache.shell != shell {
        return Err(format!(
            "login shell is now {shell}, cache was probed with {}",
            cache.shell
        ));
    }
    if cache.path.is_empty() {
        return Err("cached PATH is empty".to_string());
    }
    if let Some(reason) = first_difference(&cache.key, key) {
        return Err(reason);
    }
    // A cache stamped in the future means the clock moved, so its age is not a
    // number we can reason about at all — re-probe rather than trust it.
    let age = now
        .checked_sub(cache.probed_at)
        .map(Duration::from_secs)
        .ok_or_else(|| "cache is stamped in the future (clock changed)".to_string())?;
    if age > MAX_AGE {
        return Err(format!(
            "cache is {:.1} days old, ceiling is {:.0}",
            age.as_secs_f64() / 86_400.0,
            MAX_AGE.as_secs_f64() / 86_400.0
        ));
    }
    Ok(age)
}

/// The first way `current` disagrees with `cached`, phrased as the log line it
/// becomes. `None` when the two describe the same filesystem.
fn first_difference(cached: &[KeyEntry], current: &[KeyEntry]) -> Option<String> {
    for (was, now) in cached.iter().zip(current) {
        if was.0 != now.0 {
            return Some(format!(
                "startup files changed: expected {}, found {}",
                was.0, now.0
            ));
        }
        if was.1 != now.1 || was.2 != now.2 {
            let was_absent = was.1.is_none() && was.2.is_none();
            let now_absent = now.1.is_none() && now.2.is_none();
            return Some(match (was_absent, now_absent) {
                (true, false) => format!("{} was created", was.0),
                (false, true) => format!("{} was deleted", was.0),
                _ => format!("{} changed", was.0),
            });
        }
    }
    if cached.len() == current.len() {
        None
    } else {
        Some(format!(
            "{} startup files now, cache recorded {}",
            current.len(),
            cached.len()
        ))
    }
}

/// Everything that could change what `shell` prints for `PATH`, stat'd.
///
/// Order is fixed by construction — the per-family lists are written out and
/// directory listings are sorted — because the comparison is positional, and a
/// key whose order depended on `read_dir` would report a change on every other
/// launch.
fn path_cache_key(shell: &str) -> Vec<KeyEntry> {
    let mut key: Vec<KeyEntry> = KEY_VARS
        .iter()
        .map(|name| {
            let value = std::env::var(name).unwrap_or_default();
            (format!("env:{name}={value}"), None, None)
        })
        .collect();
    for path in candidate_files(shell) {
        key.push(stat_entry(&path));
    }
    key
}

/// The recorded state of one path: `(path, mtime_ns, size)`, or `(path, None,
/// None)` when nothing is there.
///
/// Directories are stat'd like files, which is what makes `/etc/paths.d` worth
/// naming: its own mtime moves when an entry is added or removed.
fn stat_entry(path: &Path) -> KeyEntry {
    let id = path.to_string_lossy().into_owned();
    let Ok(meta) = fs::metadata(path) else {
        return (id, None, None);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_nanos());
    (id, mtime, Some(meta.len()))
}

/// Which startup-file layout `shell` follows.
enum Family {
    Zsh,
    Bash,
    Fish,
    /// Something we have no list for. Its files are keyed as the union of zsh's
    /// and bash's, which is the safe direction to be wrong in: a few extra
    /// entries cost a `stat` each and only ever produce a *spurious* re-probe,
    /// while a missing entry produces a stale `PATH`.
    Unknown,
}

fn family(shell: &str) -> Family {
    // A login shell is conventionally invoked as `-zsh`, and `$SHELL` may carry
    // that form through; the leading dash is not part of the name.
    let name = Path::new(shell)
        .file_name()
        .map(|name| {
            name.to_string_lossy()
                .trim_start_matches('-')
                .to_lowercase()
        })
        .unwrap_or_default();
    match name.as_str() {
        "zsh" => Family::Zsh,
        "bash" => Family::Bash,
        "fish" => Family::Fish,
        _ => Family::Unknown,
    }
}

fn candidate_files(shell: &str) -> Vec<PathBuf> {
    let mut files = match family(shell) {
        Family::Zsh => zsh_files(),
        Family::Bash => bash_files(),
        Family::Fish => fish_files(),
        Family::Unknown => {
            let mut both = zsh_files();
            both.extend(bash_files());
            both
        }
    };
    files.extend(shared_files());
    // The unions above can name the same file twice, and `~/.profile` is on
    // more than one list. Duplicates would be harmless but they double the
    // `stat` and make the file confusing to read.
    let mut seen = std::collections::HashSet::new();
    files.retain(|path| seen.insert(path.clone()));
    files
}

/// `zsh -il` reads these in this order; `$ZDOTDIR` relocates the user half.
fn zsh_files() -> Vec<PathBuf> {
    let mut files = vec![
        PathBuf::from("/etc/zshenv"),
        PathBuf::from("/etc/zprofile"),
        PathBuf::from("/etc/zshrc"),
        PathBuf::from("/etc/zlogin"),
    ];
    if let Some(dir) = var_dir("ZDOTDIR").or_else(home) {
        files.extend(
            [".zshenv", ".zprofile", ".zshrc", ".zlogin"]
                .iter()
                .map(|name| dir.join(name)),
        );
    }
    files
}

/// `bash -il` reads `/etc/profile`, then the *first* of `~/.bash_profile`,
/// `~/.bash_login`, `~/.profile` that exists, and `~/.bashrc` because it is
/// interactive. All four user files are keyed rather than only the winner: an
/// existing `.bash_profile` masks `.profile`, so creating one changes the
/// answer, and only a key that already names it can notice.
fn bash_files() -> Vec<PathBuf> {
    let mut files = vec![
        PathBuf::from("/etc/profile"),
        PathBuf::from("/etc/bashrc"),
        PathBuf::from("/etc/bash.bashrc"),
    ];
    if let Some(dir) = home() {
        files.extend(
            [".bash_profile", ".bash_login", ".profile", ".bashrc"]
                .iter()
                .map(|name| dir.join(name)),
        );
    }
    files
}

/// fish keeps its `PATH` in `fish_variables` (that is where `fish_add_path`
/// writes) and sources every file under both `conf.d` directories, so the
/// entries are listed individually — a new file there is a new key entry, and
/// a key of a different length is a mismatch.
fn fish_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(dir) = var_dir("XDG_CONFIG_HOME").or_else(|| home().map(|h| h.join(".config"))) {
        let fish = dir.join("fish");
        files.push(fish.join("config.fish"));
        files.push(fish.join("fish_variables"));
        files.extend(dir_entries(&fish.join("conf.d")));
    }
    files.push(PathBuf::from("/etc/fish/config.fish"));
    files.extend(dir_entries(Path::new("/etc/fish/conf.d")));
    files
}

/// Keyed for every shell family.
///
/// `/etc/paths` and `/etc/paths.d` are macOS's `path_helper`, which runs from
/// `/etc/zprofile` and `/etc/profile` and so contributes to *every* login
/// shell's `PATH`. The rest are the state files of the version managers whose
/// shell hooks rewrite `PATH` from a value that lives outside the rc file —
/// switching the default node version edits none of the files above.
fn shared_files() -> Vec<PathBuf> {
    let mut files = vec![PathBuf::from("/etc/paths"), PathBuf::from("/etc/paths.d")];
    files.extend(dir_entries(Path::new("/etc/paths.d")));
    if let Some(nvm) = var_dir("NVM_DIR").or_else(|| home().map(|h| h.join(".nvm"))) {
        files.push(nvm.join("alias").join("default"));
    }
    if let Some(dir) = home() {
        files.extend(
            [
                ".tool-versions",
                ".python-version",
                ".node-version",
                ".ruby-version",
            ]
            .iter()
            .map(|name| dir.join(name)),
        );
    }
    files
}

/// `$HOME`, or `None` when it is unset or empty — in which case the home-side
/// entries are simply absent from the key, and `env:HOME=` records that fact so
/// the cache is invalidated once a real one appears.
fn home() -> Option<PathBuf> {
    var_dir("HOME")
}

fn var_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Every entry of `dir`, sorted; empty when the directory does not exist or
/// cannot be read, which keys exactly the same as an empty directory — and
/// that is correct, since neither contributes anything to `PATH`.
fn dir_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .collect();
    paths.sort();
    paths
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigHome;

    /// A cache that a `validate` test can then break in exactly one way.
    fn sample(key: Vec<KeyEntry>) -> PathCache {
        PathCache {
            version: CACHE_VERSION,
            shell: "/bin/zsh".to_string(),
            path: "/usr/local/bin:/usr/bin".to_string(),
            key,
            probed_at: 1_000_000,
        }
    }

    fn present(path: &str) -> KeyEntry {
        (path.to_string(), Some(42), Some(7))
    }

    fn absent(path: &str) -> KeyEntry {
        (path.to_string(), None, None)
    }

    /// The everyday case: nothing moved since the probe, so the cache answers.
    #[test]
    fn an_untouched_cache_validates_with_its_age() {
        let cache = sample(vec![present("/etc/zshrc")]);
        let age = validate(&cache, "/bin/zsh", &[present("/etc/zshrc")], 1_000_060)
            .expect("nothing changed");
        assert_eq!(age, Duration::from_mins(1));
    }

    /// The version field exists to make an old layout unreadable rather than
    /// misread, so a mismatch must reject before anything else is inspected.
    #[test]
    fn a_version_mismatch_invalidates() {
        let mut cache = sample(Vec::new());
        cache.version = CACHE_VERSION + 1;
        let err =
            validate(&cache, "/bin/zsh", &[], 1_000_000).expect_err("version must be checked");
        assert!(err.contains("version"), "unexpected reason: {err}");
    }

    /// A user who switches from zsh to fish has a different `PATH`, and none of
    /// the keyed files needs to have changed for that to be true.
    #[test]
    fn a_different_shell_invalidates() {
        let cache = sample(Vec::new());
        let err =
            validate(&cache, "/opt/homebrew/bin/fish", &[], 1_000_000).expect_err("shell changed");
        assert!(err.contains("fish"), "unexpected reason: {err}");
    }

    /// Editing an rc file is the change the key exists to catch.
    #[test]
    fn an_edited_file_invalidates() {
        let cache = sample(vec![present("/etc/zshrc")]);
        let edited = vec![("/etc/zshrc".to_string(), Some(99), Some(7))];
        let err = validate(&cache, "/bin/zsh", &edited, 1_000_000).expect_err("mtime moved");
        assert_eq!(err, "/etc/zshrc changed");
    }

    /// Absence is recorded, so a file that was never there and now is must
    /// invalidate — this is the `.zprofile` a user creates after first launch.
    #[test]
    fn a_file_that_appears_invalidates() {
        let cache = sample(vec![absent("/Users/x/.zprofile")]);
        let created = vec![present("/Users/x/.zprofile")];
        let err = validate(&cache, "/bin/zsh", &created, 1_000_000).expect_err("file appeared");
        assert_eq!(err, "/Users/x/.zprofile was created");
    }

    /// A new file in `conf.d` or `/etc/paths.d` lengthens the key rather than
    /// changing an entry, so length is compared too.
    #[test]
    fn a_longer_key_invalidates() {
        let cache = sample(vec![present("/etc/paths.d")]);
        let grown = vec![present("/etc/paths.d"), present("/etc/paths.d/40-node")];
        let err = validate(&cache, "/bin/zsh", &grown, 1_000_000).expect_err("key grew");
        assert!(err.contains("startup files"), "unexpected reason: {err}");
    }

    /// The ceiling catches what the key structurally cannot see — a new node
    /// version, a moved Homebrew prefix.
    #[test]
    fn a_cache_past_the_ttl_invalidates() {
        let cache = sample(Vec::new());
        let just_inside = cache.probed_at + MAX_AGE.as_secs();
        validate(&cache, "/bin/zsh", &[], just_inside).expect("exactly at the ceiling still holds");
        let err =
            validate(&cache, "/bin/zsh", &[], just_inside + 1).expect_err("one second past it");
        assert!(err.contains("days old"), "unexpected reason: {err}");
    }

    /// Store then load, against the real filesystem and a throwaway config
    /// directory: this is the only test that proves the two halves agree about
    /// the key, the field names, and 128-bit mtimes surviving JSON.
    #[test]
    fn a_stored_cache_loads_back() {
        let _home = ConfigHome::new();
        store("/bin/zsh", "/opt/leogit/bin:/usr/bin").expect("store");
        let hit = load("/bin/zsh").expect("a cache written a moment ago must load");
        assert_eq!(hit.path, "/opt/leogit/bin:/usr/bin");
        assert!(hit.age < Duration::from_mins(1), "age {:?}", hit.age);
        // The same file, asked about a different shell, is not an answer.
        let err = load("/bin/bash").expect_err("a different shell must miss");
        assert!(err.contains("bash"), "unexpected reason: {err}");
    }

    /// The stat side of the key: the same file, touched, must not key the same.
    /// `set_times` rather than a rewrite, so the test does not depend on the
    /// filesystem's timestamp granularity.
    #[test]
    fn touching_a_file_changes_its_key_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("zshrc");
        assert_eq!(stat_entry(&file).1, None, "a missing file keys as absent");

        fs::write(&file, b"export PATH=/x:$PATH\n").expect("write");
        let before = stat_entry(&file);
        assert_eq!(before.2, Some(21), "size is recorded");

        let handle = fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .expect("reopen");
        handle
            .set_times(
                fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(9)),
            )
            .expect("set mtime");
        let after = stat_entry(&file);
        assert_eq!(before.0, after.0, "same file, same identity");
        assert_ne!(before.1, after.1, "a moved mtime must key differently");
    }

    /// Each family keys its own rc files; an unknown one keys both lists, and
    /// every family keys `path_helper`'s inputs.
    #[test]
    fn each_shell_family_keys_its_own_startup_files() {
        let named = |shell: &str| -> Vec<String> {
            candidate_files(shell)
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        };
        // Through a binding rather than a literal, so the suffix reads as the
        // tail of a path — which it is — rather than as a file extension.
        let keys = |paths: &[String], suffix: &str| paths.iter().any(|path| path.ends_with(suffix));

        let zsh = named("/bin/zsh");
        assert!(keys(&zsh, ".zshrc"));
        assert!(!keys(&zsh, ".bashrc"));

        let bash = named("/bin/bash");
        assert!(keys(&bash, ".bash_profile"));
        assert!(!keys(&bash, ".zshrc"));

        let fish = named("/opt/homebrew/bin/fish");
        assert!(keys(&fish, "fish/fish_variables"));

        let unknown = named("/usr/local/bin/nu");
        assert!(keys(&unknown, ".zshrc"));
        assert!(keys(&unknown, ".bashrc"));

        for shell in ["/bin/zsh", "/bin/bash", "/opt/homebrew/bin/fish", "/bin/nu"] {
            assert!(
                named(shell).iter().any(|p| p == "/etc/paths"),
                "{shell} must key path_helper's input"
            );
        }
    }

    /// The variables that decide *which* files are keyed are keyed themselves,
    /// and a login shell invoked as `-zsh` is still zsh.
    #[test]
    fn the_key_records_the_variables_that_shape_it() {
        let key = path_cache_key("-zsh");
        for name in KEY_VARS {
            assert!(
                key.iter()
                    .any(|(id, _, _)| id.starts_with(&format!("env:{name}="))),
                "{name} must be in the key"
            );
        }
        let zshrc = ".zshrc";
        assert!(
            key.iter().any(|(id, _, _)| id.ends_with(zshrc)),
            "`-zsh` must be recognised as zsh"
        );
    }
}

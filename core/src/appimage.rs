//! Undoing the `AppImage` runtime environment for the processes we spawn.
//!
//! On Linux `LeoGit` ships as an `AppImage`, and its `AppRun` wrapper exports a set
//! of variables pointing into the temporary mount the image unpacks itself into
//! — `$APPDIR`, e.g. `/tmp/.mount_leogitXXXXXX`. `LD_LIBRARY_PATH`,
//! `GTK_PATH`, `GIO_EXTRA_MODULES`, `GDK_PIXBUF_MODULE_FILE`, `QT_PLUGIN_PATH`,
//! `PYTHONHOME`, `PERLLIB`, a `PATH` prefix and more all point in there.
//!
//! Those values are correct for *this* process and must stay: they are how the
//! bundled `WebKitGTK`, its own helper processes, and every GTK module we load
//! lazily find their libraries. Scrubbing our own environment would break the
//! app, so this module never touches it.
//!
//! They are wrong for every program we hand off to. `git`, `gh`, `claude`, the
//! shells in the terminal panel and whatever `xdg-open` launches are system
//! binaries that must link against system libraries; handed our bundle's paths
//! they load the wrong `libssl` or fail outright. And because the mount is
//! unmounted when `LeoGit` quits, a child that outlives us — an editor opened
//! from a file row, a terminal — is left pointing at paths that no longer
//! exist. A `PYTHONHOME` under a dead mount stops `python3` from starting at
//! all: it cannot find `encodings` and aborts before running a line.
//!
//! ## The policy
//!
//! The edits are derived from `$APPDIR` rather than from a list of variable
//! names, because the list is not ours to keep current — every `linuxdeploy`
//! plugin adds its own, and a name we failed to anticipate is exactly the one
//! that would leak. So: drop every `:`-separated entry that lives under
//! `$APPDIR`, drop the variable entirely when nothing real is left, and leave
//! everything that never mentioned `$APPDIR` untouched.
//!
//! Edits are recomputed per spawn rather than cached, because the environment
//! legitimately changes once at startup: [`crate::process::fix_path_env`]
//! replaces `PATH` after probing the login shell, and a snapshot taken before
//! that would re-apply the stale `PATH` to every child forever after.
//! Recomputing costs a scan of ~60 short strings against a `fork`/`exec`.

use std::sync::Once;

/// Variables the `AppImage` runtime invents to describe itself. They name no path
/// that filtering could clean, and a child that reads them believes it is
/// running inside an `AppImage` of its own, so they go entirely.
const MARKERS: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "APPIMAGE_EXTRACT_AND_RUN",
    "ARGV0",
    "OWD",
];

/// Filtered like any other list but never deleted: a child with no `PATH` at
/// all falls back to whatever the C library invents, which is worse than the
/// stale value this module exists to fix. Unreachable in practice — `AppRun`
/// prepends to the session `PATH` rather than replacing it — and cheap to
/// guarantee.
const NEVER_REMOVED: &[&str] = &["PATH"];

/// One edit to a child's environment: `Some` replaces the value, `None` removes
/// the variable.
pub type EnvEdit = (String, Option<String>);

/// A command builder whose child environment can be edited.
///
/// Implemented for each of the three command types `LeoGit` spawns through so
/// that [`sanitize`] states the policy once instead of once per builder.
pub trait ChildEnv {
    /// Set `key` to `value` in the child's environment.
    fn set_var(&mut self, key: &str, value: &str);
    /// Remove `key` from the child's environment.
    fn remove_var(&mut self, key: &str);
}

impl ChildEnv for std::process::Command {
    fn set_var(&mut self, key: &str, value: &str) {
        self.env(key, value);
    }
    fn remove_var(&mut self, key: &str) {
        self.env_remove(key);
    }
}

impl ChildEnv for tokio::process::Command {
    fn set_var(&mut self, key: &str, value: &str) {
        self.env(key, value);
    }
    fn remove_var(&mut self, key: &str) {
        self.env_remove(key);
    }
}

// `CommandBuilder` materializes the inherited environment into a map at
// construction, so removing a key here really does keep it from the child
// rather than merely shadowing it.
impl ChildEnv for portable_pty::CommandBuilder {
    fn set_var(&mut self, key: &str, value: &str) {
        self.env(key, value);
    }
    fn remove_var(&mut self, key: &str) {
        self.env_remove(key);
    }
}

/// Strip this process's `AppImage` environment from `cmd`'s child environment.
/// A no-op when `LeoGit` is not running from an `AppImage`, which is every case
/// off Linux and most cases on it.
pub fn sanitize<C: ChildEnv + ?Sized>(cmd: &mut C) {
    apply(cmd, child_env_edits());
}

/// Apply `edits` to `cmd`. Separate from [`sanitize`] so a test can drive the
/// builder plumbing with an edit list of its own instead of whatever
/// environment it happens to run in.
fn apply<C: ChildEnv + ?Sized>(cmd: &mut C, edits: Vec<EnvEdit>) {
    for (key, value) in edits {
        match value {
            Some(v) => cmd.set_var(&key, &v),
            None => cmd.remove_var(&key),
        }
    }
}

/// The edits that undo this process's `AppImage` environment, empty when we are
/// not running from one.
#[must_use]
pub fn child_env_edits() -> Vec<EnvEdit> {
    // Logged once per run: enough to confirm the scrub is active and name what
    // it touched, without a line per `git status` poll.
    static LOGGED: Once = Once::new();

    let Some(root) = appdir() else {
        return Vec::new();
    };
    // `vars()` panics on a non-UTF-8 value; a child's environment is not worth
    // taking the app down for, so those pairs are skipped instead. They cannot
    // hold an `$APPDIR` path anyway — the mount point is ASCII.
    let vars = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    let edits = edits_for(&root, vars);

    LOGGED.call_once(|| {
        let names: Vec<&str> = edits.iter().map(|(k, _)| k.as_str()).collect();
        println!(
            "[appimage] APPDIR={root} — scrubbing {} vars from child processes: {}",
            names.len(),
            names.join(", ")
        );
    });

    edits
}

/// `$APPDIR` when this process was started by an `AppImage`'s `AppRun`, and the
/// value is usable as a path prefix.
fn appdir() -> Option<String> {
    // AppImages are a Linux packaging format; the other clients never pay for
    // this beyond a branch the optimizer folds away.
    if !cfg!(target_os = "linux") {
        return None;
    }
    let dir = std::env::var("APPDIR").ok()?;
    let dir = dir.trim_end_matches('/');
    // An absolute path with at least one component. A relative or empty
    // `$APPDIR` is not something we can match entries against, and treating it
    // as a prefix would match far too much.
    (dir.starts_with('/') && dir.len() > 1).then(|| dir.to_string())
}

/// Compute the edits for `appdir` over `vars`.
///
/// Split out from [`child_env_edits`] so the policy is testable without an
/// `AppImage` to run inside. `appdir` must already be trimmed of trailing `/`.
fn edits_for(appdir: &str, vars: impl Iterator<Item = (String, String)>) -> Vec<EnvEdit> {
    let mut edits = Vec::new();
    for (key, value) in vars {
        if MARKERS.contains(&key.as_str()) {
            edits.push((key, None));
            continue;
        }
        // Untouched by the `AppImage`: leave it byte-identical rather than
        // round-tripping it through a split/join that could alter it.
        if !value.contains(appdir) {
            continue;
        }
        let kept: Vec<&str> = value
            .split(':')
            .filter(|entry| !is_under(appdir, entry))
            .collect();
        // Only empty entries left means the variable held nothing but `AppImage`
        // paths — including the trailing `:` `AppRun` leaves when it prepends to
        // a variable the session did not set (`PYTHONPATH=$APPDIR/...:`).
        if kept.iter().all(|entry| entry.is_empty()) {
            if NEVER_REMOVED.contains(&key.as_str()) {
                continue;
            }
            edits.push((key, None));
        } else {
            let joined = kept.join(":");
            // The value mentioned `$APPDIR` only as a substring of some other
            // path — a sibling mount, say. Nothing was dropped, so there is no
            // edit to make.
            if joined == value {
                continue;
            }
            edits.push((key, Some(joined)));
        }
    }
    edits
}

/// Whether `entry` is `appdir` itself or a path inside it. Compared by path
/// component so a sibling mount (`/tmp/.mount_leogitAB` next to
/// `/tmp/.mount_leogitA`) is not mistaken for a child.
fn is_under(appdir: &str, entry: &str) -> bool {
    let trimmed = entry.trim_end_matches('/');
    trimmed == appdir
        || entry
            .strip_prefix(appdir)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPDIR: &str = "/tmp/.mount_leogitXXX";

    fn edits(vars: &[(&str, &str)]) -> Vec<EnvEdit> {
        edits_for(
            APPDIR,
            vars.iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        )
    }

    // The failure that motivates this module: a `PYTHONHOME` under a mount that
    // disappears when `LeoGit` quits stops `python3` starting at all.
    #[test]
    fn removes_a_variable_that_is_only_an_appimage_path() {
        assert_eq!(
            edits(&[("PYTHONHOME", "/tmp/.mount_leogitXXX/usr/")]),
            [("PYTHONHOME".to_string(), None)]
        );
    }

    // `AppRun` prepends its own entry and keeps the session's value after it; the
    // session's half is the part the child actually wants.
    #[test]
    fn keeps_the_session_entries_of_a_prepended_list() {
        assert_eq!(
            edits(&[(
                "XDG_DATA_DIRS",
                "/tmp/.mount_leogitXXX/usr/share/:/usr/share:/usr/local/share"
            )]),
            [(
                "XDG_DATA_DIRS".to_string(),
                Some("/usr/share:/usr/local/share".to_string())
            )]
        );
    }

    // When the session did not set the variable at all, `AppRun` still appends the
    // separator — `$APPDIR/...:` — and the empty remainder must not be handed to
    // the child as an empty value (to Python an empty entry means the cwd).
    #[test]
    fn removes_a_list_whose_only_survivor_is_the_empty_original() {
        assert_eq!(
            edits(&[("PYTHONPATH", "/tmp/.mount_leogitXXX/usr/share/pyshared/:")]),
            [("PYTHONPATH".to_string(), None)]
        );
    }

    // Doubled separators are how linuxdeploy's own hooks write these
    // (`$APPDIR//usr/lib/...`), so the prefix match has to tolerate them.
    #[test]
    fn matches_entries_written_with_a_doubled_separator() {
        assert_eq!(
            edits(&[(
                "GDK_PIXBUF_MODULE_FILE",
                "/tmp/.mount_leogitXXX//usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
            )]),
            [("GDK_PIXBUF_MODULE_FILE".to_string(), None)]
        );
    }

    #[test]
    fn leaves_variables_that_never_mention_the_appdir() {
        assert!(edits(&[("HOME", "/home/leo"), ("EDITOR", "vim")]).is_empty());
    }

    // A concurrently mounted `AppImage` whose directory shares our prefix is a
    // different mount, and its entries are none of our business.
    #[test]
    fn does_not_match_a_sibling_mount_sharing_the_prefix() {
        assert!(edits(&[("LD_LIBRARY_PATH", "/tmp/.mount_leogitXXXOTHER/usr/lib")]).is_empty());
    }

    #[test]
    fn removes_the_appimage_markers_outright() {
        let out = edits(&[
            ("APPDIR", APPDIR),
            ("APPIMAGE", "/home/leo/.local/bin/leogit.AppImage"),
            ("OWD", "/home/leo"),
            ("ARGV0", "leogit"),
        ]);
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|(_, value)| value.is_none()));
    }

    // PATH is filtered like anything else...
    #[test]
    fn filters_path_but_keeps_the_session_entries() {
        assert_eq!(
            edits(&[(
                "PATH",
                "/home/leo/.local/bin:/tmp/.mount_leogitXXX/usr/bin/:/usr/bin"
            )]),
            [(
                "PATH".to_string(),
                Some("/home/leo/.local/bin:/usr/bin".to_string())
            )]
        );
    }

    // ...but is never deleted: no PATH is worse for a child than a stale one.
    #[test]
    fn never_removes_path_entirely() {
        assert!(edits(&[("PATH", "/tmp/.mount_leogitXXX/usr/bin")]).is_empty());
    }

    // Off an `AppImage` there is nothing to undo, and every spawn site must be
    // able to call the sanitizer unconditionally.
    #[test]
    fn no_edits_when_not_running_from_an_appimage() {
        if std::env::var_os("APPDIR").is_none() {
            assert!(child_env_edits().is_empty());
        }
    }

    // The policy above is only worth anything if a removal actually reaches the
    // child, so this asks a real one what it received.
    #[cfg(unix)]
    #[test]
    fn a_removal_reaches_a_spawned_child() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "printf %s ${LEOGIT_TEST_VAR-gone}"]);
        cmd.env("LEOGIT_TEST_VAR", "under-the-mount");
        apply(&mut cmd, vec![("LEOGIT_TEST_VAR".to_string(), None)]);
        let out = cmd.output().expect("sh must run");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "gone");
    }

    // `CommandBuilder` materializes the inherited environment at construction
    // rather than deferring to the child, which is what makes `env_remove` a
    // real removal here. Assert that, because the terminal panel depends on it.
    #[test]
    fn a_removal_drops_an_inherited_variable_from_a_pty_command() {
        let mut cmd = portable_pty::CommandBuilder::new("sh");
        assert!(cmd.get_env("PATH").is_some(), "PATH should be inherited");
        apply(&mut cmd, vec![("PATH".to_string(), None)]);
        assert!(cmd.get_env("PATH").is_none());
        assert!(cmd.iter_full_env_as_str().all(|(key, _)| key != "PATH"));
    }
}

//! One home for turning a filesystem path into the form the rest of the app
//! uses: absolute, symlink-resolved, and — on Windows — the *legacy* form
//! rather than the verbatim one.
//!
//! `std::fs::canonicalize` on Windows always answers with a verbatim
//! (extended-length) path: `\\?\C:\Users\Leo\Dev\leogit`. It names the right
//! folder and the Win32 API accepts it, but it leaks badly:
//!
//! * every place the UI shows a repo path — the picker's hover tooltip, the
//!   "no repositories found" state, error messages — read `\\?\C:\…`;
//! * `PowerShell` can't map a verbatim path onto a `PSDrive`, so a shell started
//!   there falls back to a provider-qualified prompt
//!   (`PS Microsoft.PowerShell.Core\FileSystem::\\?\C:\…`), and anything that
//!   does string work on `$PWD` sees that instead of a path;
//! * plenty of third-party tools simply don't parse the prefix.
//!
//! So paths are converted once, where they enter the app, and every consumer
//! gets the compatible form. Conversion is [`dunce`]'s: it only strips the
//! prefix for a drive-letter path that the legacy namespace can express — not
//! one over `MAX_PATH`, not one holding a reserved DOS name (`CON`, `COM1`),
//! not a network share — so nothing that needs the prefix loses it.
//!
//! **macOS and Linux are untouched by design.** `dunce`'s strip check is a
//! `const fn` returning `false` off Windows and its `canonicalize` is a
//! re-export of `std::fs::canonicalize` there, so both functions below are the
//! std behaviour verbatim on those platforms.
//!
//! [`expand_tilde`] lives here for the same reason: `~` is the other notation
//! the app has to turn into a real path before anything can open it.

use std::io;
use std::path::{Path, PathBuf};

/// Resolve `path` to an absolute, symlink-free path in the most compatible
/// form for the platform. Drop-in replacement for [`std::fs::canonicalize`],
/// and identical to it off Windows.
///
/// This is the only canonicalizer the app should call, so no code path can
/// hand a verbatim path to the UI or to a shell.
///
/// # Errors
/// Same as [`std::fs::canonicalize`]: the path must exist and be readable.
pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// Rewrite an *already resolved* path into the same compatible form, without
/// touching the filesystem.
///
/// [`canonicalize`] covers paths we resolve ourselves; this covers paths that
/// arrive already-absolute from somewhere else — a state file written by an
/// older build, a git subprocess — where the folder may not even exist any
/// more and so can't be re-resolved.
///
/// Idempotent: a path that's already in the compatible form is returned
/// unchanged, which is also what every path on macOS and Linux gets.
#[must_use]
pub fn simplify_str(path: &str) -> String {
    dunce::simplified(Path::new(path))
        .to_string_lossy()
        .into_owned()
}

/// Expand a leading `~` to the user's home directory. Any other path — and a
/// `~` that isn't the whole first component, like `~backup/notes` — is returned
/// as written.
///
/// The home directory comes from the *OS*, never from `$HOME` alone. That
/// variable isn't part of the environment Windows hands a program: Explorer and
/// the Start menu don't set it, and only POSIX-flavoured shells (Git Bash,
/// MSYS) do. Reading it directly therefore made `~` resolve or silently fail
/// depending on how the app happened to be launched — scan paths worked under
/// `tauri dev` from such a shell and found nothing at all in an installed
/// build, where discovery skipped every root it couldn't resolve.
///
/// [`directories`] asks for `FOLDERID_Profile` on Windows, which no
/// environment can misreport, and reads `$HOME` on macOS and Linux exactly as
/// before (falling back to the passwd entry), so the fix is Windows-only in
/// effect without a `cfg` of ours.
#[must_use]
pub fn expand_tilde(path: &str) -> PathBuf {
    expand_tilde_from(path, home_dir().as_deref())
}

/// The user's home directory, resolved the same way [`expand_tilde`] resolves
/// `~`. Also a search root in its own right (see [`super::repos`]), so a repo
/// outside every scan folder is still shown and matched by the part of its
/// path that distinguishes it.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// The rule itself, split out so it can be tested against a known home
/// directory instead of mutating the process environment.
fn expand_tilde_from(path: &str, home: Option<&Path>) -> PathBuf {
    let literal = || PathBuf::from(path);
    let Some(rest) = path.strip_prefix('~') else {
        return literal();
    };
    // `~` alone or `~` followed by a separator names the home directory;
    // `~user` and a folder actually called `~snapshots` do not.
    if !rest.is_empty() && !rest.starts_with(std::path::is_separator) {
        return literal();
    }
    let Some(home) = home else {
        return literal();
    };
    // Push component by component rather than joining the remainder wholesale:
    // that accepts either separator (a Windows user may well type `~\Dev`) and
    // emits the platform's own, so the result doesn't come out half `\` and
    // half `/` — a form that reads as broken and, worse, compares unequal to
    // the same folder discovered any other way.
    let mut expanded = home.to_path_buf();
    expanded.extend(
        rest.split(std::path::is_separator)
            .filter(|component| !component.is_empty()),
    );
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Nothing that isn't a Windows verbatim path may be rewritten — this is
    /// what keeps macOS and Linux out of the conversion entirely, since no path
    /// there can match.
    #[test]
    fn ordinary_paths_pass_through_unchanged() {
        for path in [
            "/Users/leo/Dev/leogit",
            "/home/leo/code/app",
            r"C:\Users\Leo\Dev\leogit",
            "relative/dir",
            "",
        ] {
            assert_eq!(simplify_str(path), path, "must not rewrite {path:?}");
        }
    }

    /// The bug this module exists for: the prefix the UI and `PowerShell` were
    /// both choking on is gone.
    #[cfg(windows)]
    #[test]
    fn verbatim_disk_paths_lose_the_prefix() {
        assert_eq!(
            simplify_str(r"\\?\C:\Users\Leo\Dev\leogit"),
            r"C:\Users\Leo\Dev\leogit"
        );
        assert_eq!(simplify_str(r"\\?\c:\a"), r"c:\a");
    }

    /// Re-simplifying is a no-op, so running the state migration on every read
    /// can't corrupt a path that was already converted.
    #[cfg(windows)]
    #[test]
    fn simplifying_twice_changes_nothing() {
        let once = simplify_str(r"\\?\C:\Users\Leo\Dev\leogit");
        assert_eq!(simplify_str(&once), once);
    }

    /// Paths the legacy namespace genuinely can't express keep their prefix —
    /// stripping these would produce a path that opens the wrong thing, or
    /// nothing at all.
    #[cfg(windows)]
    #[test]
    fn paths_that_need_the_prefix_keep_it() {
        // `CON` is a DOS device, not a folder, once the prefix is gone.
        let reserved = r"\\?\C:\Dev\CON";
        assert_eq!(simplify_str(reserved), reserved);
        // A network share has no drive letter to fall back to.
        let share = r"\\?\UNC\server\share\repo";
        assert_eq!(simplify_str(share), share);
        // Over MAX_PATH, the prefix is the only way to name it.
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(simplify_str(&long), long);
    }

    /// `~` is the notation the settings dialog and the stock scan paths are
    /// written in, so it has to land on the home directory in the platform's
    /// own separators.
    #[test]
    fn tilde_expands_to_the_home_directory() {
        let home = Path::new(if cfg!(windows) {
            r"C:\Users\Leo"
        } else {
            "/Users/leo"
        });
        let expand = |path: &str| expand_tilde_from(path, Some(home));

        assert_eq!(expand("~"), home);
        assert_eq!(
            expand("~/Dev/LeoManrique"),
            home.join("Dev").join("LeoManrique")
        );
        // Trailing and doubled separators are typos the settings textarea will
        // see; neither should produce an empty component.
        assert_eq!(expand("~/Dev/"), home.join("Dev"));
        assert_eq!(expand("~//Dev"), home.join("Dev"));
    }

    /// A Windows user types the separator their OS uses, and the result must
    /// still be one consistent path rather than `C:\Users\Leo\Dev/sub`.
    #[cfg(windows)]
    #[test]
    fn tilde_expansion_accepts_and_emits_windows_separators() {
        let home = Path::new(r"C:\Users\Leo");
        assert_eq!(
            expand_tilde_from(r"~\Dev\LeoManrique", Some(home)),
            Path::new(r"C:\Users\Leo\Dev\LeoManrique")
        );
        assert_eq!(
            expand_tilde_from("~/Dev/LeoManrique", Some(home)).to_string_lossy(),
            r"C:\Users\Leo\Dev\LeoManrique"
        );
    }

    /// Only a whole leading component counts. `~user` is another user's home,
    /// which we don't resolve, and a folder may simply be named with a `~`.
    #[test]
    fn other_uses_of_a_tilde_are_left_alone() {
        let home = Path::new("/Users/leo");
        for path in ["~backup/notes", "~user", "/Dev/~snapshots", "Dev/~", ""] {
            assert_eq!(
                expand_tilde_from(path, Some(home)),
                PathBuf::from(path),
                "must not expand {path:?}"
            );
        }
    }

    /// With no home directory to expand against, the path stays literal rather
    /// than becoming an absolute path rooted at nothing.
    #[test]
    fn tilde_without_a_home_directory_stays_literal() {
        assert_eq!(expand_tilde_from("~/Dev", None), PathBuf::from("~/Dev"));
    }

    /// The wiring, not just the rule: the OS lookup has to actually answer, or
    /// every `~` scan path silently expands to nothing — which is precisely
    /// what an installed Windows build did while reading `$HOME`.
    #[test]
    fn tilde_expands_against_the_real_home_directory() {
        let expanded = expand_tilde("~/Dev");

        assert!(
            expanded.is_absolute(),
            "`~` must resolve from the OS, not the environment: {}",
            expanded.display()
        );
        assert!(expanded.ends_with("Dev"), "{}", expanded.display());
    }

    /// The resolver behind every repo path the app hands out: it still resolves
    /// (absolute, `.` collapsed) and never yields a verbatim path for an
    /// ordinary folder.
    #[test]
    fn canonicalize_resolves_without_producing_a_verbatim_path() {
        let tmp = tempdir().expect("tempdir");
        let nested = tmp.path().join("repos/project");
        std::fs::create_dir_all(&nested).expect("create nested dirs");

        let resolved = canonicalize(nested.join(".")).expect("canonicalize");

        assert!(resolved.is_absolute(), "must resolve to an absolute path");
        assert!(
            resolved.ends_with("project"),
            "must still point at the folder: {}",
            resolved.display()
        );
        assert!(
            !resolved.to_string_lossy().starts_with(r"\\?\"),
            "an ordinary folder must not come back verbatim: {}",
            resolved.display()
        );
    }
}

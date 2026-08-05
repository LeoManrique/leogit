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

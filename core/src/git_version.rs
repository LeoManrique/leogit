//! The installed git's version, and the floor `LeoGit` holds it to.
//!
//! `LeoGit` runs the system git and tracks recent versions of it only: the
//! floor is the newest flag the core relies on, and it rises whenever a newer
//! git has something worth using. There is no degraded mode below it and no
//! second spelling of any flag — one check, one sentence.

use super::process;
use std::fmt;
use std::process::Command;
use std::sync::OnceLock;

/// The oldest git `LeoGit` supports — `cherry-pick --empty=keep` arrived in it.
pub const FLOOR: GitVersion = GitVersion {
    major: 2,
    minor: 45,
    patch: 0,
};

/// A git release, as far as ordering needs it: `2.54.0`.
///
/// Derives its ordering, so the fields stay in significance order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl GitVersion {
    /// Read a version out of what `git --version` prints.
    ///
    /// Every vendor decorates the line differently — `git version 2.54.0
    /// (Apple Git-157)`, `2.46.0.windows.1`, `2.47.0-rc1`, a source build's
    /// `2.54.GIT` — so only the leading numbers of the third word are read, and
    /// a component that is missing or not a number counts as zero. Major and
    /// minor must both be there: a line without them is not a version.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let word = line.trim().strip_prefix("git version ")?;
        let word = word.split_whitespace().next()?;
        let mut numbers = word.split('.').map(|part| {
            let digits = part
                .find(|c: char| !c.is_ascii_digit())
                .map_or(part, |end| &part[..end]);
            digits.parse::<u32>().ok()
        });
        let major = numbers.next()??;
        let minor = numbers.next()??;
        let patch = numbers.next().flatten().unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

/// The git on `PATH`, asked once per process.
///
/// Only an answer is remembered: a git that could not be run or read is asked
/// again next time, so installing one does not need a relaunch to be noticed.
///
/// # Errors
/// When git cannot be started, or prints a version line this cannot read.
pub fn installed() -> Result<GitVersion, String> {
    static INSTALLED: OnceLock<GitVersion> = OnceLock::new();
    if let Some(version) = INSTALLED.get() {
        return Ok(*version);
    }
    let mut cmd = Command::new("git");
    cmd.arg("--version");
    process::prepare_child(&mut cmd);
    let output = cmd.output().map_err(|e| format!("git --version: {e}"))?;
    let line = String::from_utf8_lossy(&output.stdout);
    let version = GitVersion::parse(&line)
        .ok_or_else(|| format!("Could not read git's version from \"{}\".", line.trim()))?;
    Ok(*INSTALLED.get_or_init(|| version))
}

/// The one sentence a git below the floor is refused with, or `Ok` at or above
/// it. Split from [`require_floor`] so the wording is testable without a git
/// of every age on the machine.
///
/// # Errors
/// When `version` is older than [`FLOOR`].
pub fn check_floor(version: GitVersion) -> Result<(), String> {
    if version >= FLOOR {
        return Ok(());
    }
    Err(format!(
        "LeoGit needs git {FLOOR} or newer for this; the git on this machine is {version}."
    ))
}

/// Refuse when the installed git is below [`FLOOR`].
///
/// # Errors
/// When git cannot be asked for its version, or it is older than the floor.
pub fn require_floor() -> Result<(), String> {
    check_floor(installed()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u32, minor: u32, patch: u32) -> GitVersion {
        GitVersion {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn parses_every_vendors_version_line() {
        let cases = [
            ("git version 2.54.0 (Apple Git-157)", version(2, 54, 0)),
            ("git version 2.45.2\n", version(2, 45, 2)),
            ("git version 2.46.0.windows.1", version(2, 46, 0)),
            ("git version 2.47.0-rc1", version(2, 47, 0)),
            ("git version 2.54.GIT", version(2, 54, 0)),
            ("git version 3.0", version(3, 0, 0)),
        ];
        for (line, expected) in cases {
            assert_eq!(GitVersion::parse(line), Some(expected), "{line}");
        }
    }

    #[test]
    fn a_line_without_a_major_and_minor_is_not_a_version() {
        for line in [
            "",
            "git version",
            "git version next",
            "git version 2",
            "hg 6.1",
        ] {
            assert_eq!(GitVersion::parse(line), None, "{line:?}");
        }
    }

    #[test]
    fn preflight_refuses_a_git_below_the_floor() {
        let refusal = check_floor(version(2, 44, 9)).expect_err("2.44 is below the floor");
        assert!(
            refusal.contains("2.45.0") && refusal.contains("2.44.9"),
            "names both versions: {refusal}"
        );
        assert!(check_floor(FLOOR).is_ok(), "the floor itself passes");
        assert!(check_floor(version(2, 54, 0)).is_ok());
        assert!(check_floor(version(3, 0, 0)).is_ok(), "a new major passes");
    }

    #[test]
    fn the_installed_git_is_readable_and_meets_the_floor() {
        let installed = installed().expect("git --version");
        assert!(
            check_floor(installed).is_ok(),
            "the development machine runs a supported git, got {installed}"
        );
    }
}

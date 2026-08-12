//! Shell discovery for the embedded terminal.
//!
//! The terminal panel needs to answer two questions: *which shells can we
//! actually launch on this machine*, and *which one did the user pick*. Both
//! live here so `terminal.rs` only deals with the PTY itself.
//!
//! Discovery is probe-based rather than name-based: every candidate is only
//! offered once its executable has been found on disk, so the picker can never
//! list a shell that would fail to spawn. The list is ordered best-first, and
//! `resolve` falls back to that order whenever the stored preference names a
//! shell that has since been uninstalled.

use serde::{Deserialize, Serialize};
use std::path::Path;
// `PathBuf` is only needed by the Windows-only Git-install probing below.
#[cfg(windows)]
use std::path::PathBuf;

/// A launchable shell: what to exec, how to label it, and the argv it needs to
/// come up as an interactive session in the repo directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellOption {
    /// Stable identifier persisted in `config.terminal_shell`. Never
    /// localized and never a filesystem path, so moving a Git install doesn't
    /// invalidate the user's choice.
    pub id: String,
    /// Human-readable name for the picker.
    pub label: String,
    /// Absolute path to the executable.
    pub path: String,
    /// Arguments appended after the executable.
    pub args: Vec<String>,
}

/// Return the first candidate path that exists on disk.
fn first_existing<I, P>(candidates: I) -> Option<String>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    candidates
        .into_iter()
        .find(|p| p.as_ref().is_file())
        .map(|p| p.as_ref().to_string_lossy().into_owned())
}

/// Find an executable by name on `PATH`, returning its full path if present.
/// Used to probe for a shell before offering it, so we never hand the PTY a
/// name that doesn't resolve. Only the Windows discovery path probes `PATH`
/// this way; the Unix path uses absolute candidates via [`first_existing`].
#[cfg(windows)]
fn which_on_path(exe: &str) -> Option<String> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

/// Root of the Git for Windows installation, preferring the location the
/// installer recorded in the registry over the well-known directories. A
/// user who installed Git somewhere non-standard is still found this way.
#[cfg(windows)]
fn git_install_root() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let from_registry = [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .filter_map(|hive| {
            RegKey::predef(hive)
                .open_subkey(r"SOFTWARE\GitForWindows")
                .ok()?
                .get_value::<String, _>("InstallPath")
                .ok()
        })
        .map(PathBuf::from)
        .find(|root| root.is_dir());

    if let Some(root) = from_registry {
        return Some(root);
    }

    // No registry entry (portable install, or Git installed for another user):
    // fall back to the default install locations, then to wherever `git.exe`
    // resolved from — `<root>/cmd/git.exe` means `<root>` is the install root.
    let defaults = [
        std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Git")),
        std::env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Git")),
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join("Git")),
    ];
    defaults
        .into_iter()
        .flatten()
        .find(|root| root.is_dir())
        .or_else(|| {
            let git = PathBuf::from(which_on_path("git.exe")?);
            // <root>/cmd/git.exe or <root>/bin/git.exe
            git.parent()?.parent().map(Path::to_path_buf)
        })
}

/// Shells worth offering on Windows, best-first.
///
/// Git Bash leads because it is the shell most git workflows assume: POSIX
/// tooling, and the one every `git`-flavoured snippet on the internet is
/// written for. PowerShell 7 beats Windows PowerShell 5.1 because 5.1 ships a
/// 2019-era `PSReadLine` whose `ConPTY` repaint handling is visibly worse.
#[cfg(windows)]
fn discover() -> Vec<ShellOption> {
    let mut shells = Vec::new();

    if let Some(root) = git_install_root() {
        // `bin/bash.exe` is the launcher that sets up the MSYS environment;
        // `usr/bin/bash.exe` is the raw binary and needs MSYSTEM set by hand.
        // Prefer the former and let `/etc/profile` do the work.
        if let Some(path) = first_existing([root.join("bin").join("bash.exe")]) {
            shells.push(ShellOption {
                id: "git-bash".into(),
                label: "Git Bash".into(),
                path,
                // `--login` sources /etc/profile, which is what populates PATH
                // with the MSYS tools. Git for Windows' profile does not `cd`
                // to $HOME (only the `git-bash.exe --cd-to-home` shortcut
                // does), so the PTY's cwd survives.
                args: vec!["--login".into(), "-i".into()],
            });
        }
    }

    if let Some(path) = which_on_path("pwsh.exe") {
        shells.push(ShellOption {
            id: "pwsh".into(),
            label: "PowerShell".into(),
            path,
            args: vec!["-NoLogo".into()],
        });
    }

    if let Some(path) = which_on_path("powershell.exe") {
        shells.push(ShellOption {
            id: "windows-powershell".into(),
            label: "Windows PowerShell".into(),
            path,
            args: vec!["-NoLogo".into()],
        });
    }

    // COMSPEC is effectively guaranteed, but fall back to the bare name so the
    // list is never empty and the terminal always has something to launch.
    let cmd_path = std::env::var("COMSPEC")
        .ok()
        .filter(|p| Path::new(p).is_file())
        .or_else(|| which_on_path("cmd.exe"))
        .unwrap_or_else(|| "cmd.exe".into());
    shells.push(ShellOption {
        id: "cmd".into(),
        label: "Command Prompt".into(),
        path: cmd_path,
        args: Vec::new(),
    });

    shells
}

/// Shells worth offering on Unix, best-first.
///
/// `$SHELL` leads: it is the user's own login shell and the only value that
/// respects their `chsh`. The named entries below it exist so the picker can
/// offer alternatives, and are de-duplicated against `$SHELL` by path.
#[cfg(not(windows))]
fn discover() -> Vec<ShellOption> {
    let mut shells = Vec::new();

    if let Some(path) = std::env::var("SHELL")
        .ok()
        .filter(|p| Path::new(p).is_file())
    {
        let label = Path::new(&path).file_name().map_or_else(
            || "Default shell".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        shells.push(ShellOption {
            id: "default".into(),
            label: format!("{label} (login shell)"),
            path,
            args: Vec::new(),
        });
    }

    for (id, label, candidates) in [
        (
            "zsh",
            "zsh",
            ["/bin/zsh", "/usr/bin/zsh", "/usr/local/bin/zsh"],
        ),
        (
            "bash",
            "bash",
            ["/bin/bash", "/usr/bin/bash", "/opt/homebrew/bin/bash"],
        ),
        (
            "fish",
            "fish",
            [
                "/usr/local/bin/fish",
                "/usr/bin/fish",
                "/opt/homebrew/bin/fish",
            ],
        ),
        ("sh", "sh", ["/bin/sh", "/usr/bin/sh", "/usr/bin/sh"]),
    ] {
        if let Some(path) = first_existing(candidates) {
            if shells.iter().any(|s| s.path == path) {
                continue;
            }
            shells.push(ShellOption {
                id: id.into(),
                label: label.into(),
                path,
                args: Vec::new(),
            });
        }
    }

    shells
}

/// Last-resort shell for a machine where discovery found nothing. Both
/// `discover` arms already guarantee a fallback, so this only exists to keep
/// [`available`] and [`resolve`] total — neither should be able to panic just
/// because a system is unusual.
fn fallback_shell() -> ShellOption {
    ShellOption {
        id: if cfg!(windows) { "cmd" } else { "sh" }.into(),
        label: if cfg!(windows) {
            "Command Prompt"
        } else {
            "sh"
        }
        .into(),
        path: if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into(),
        args: Vec::new(),
    }
}

/// Every shell that can actually be launched on this machine, best-first.
///
/// Never empty: on Windows `cmd.exe` is always appended, and on Unix `/bin/sh`
/// is the floor. Callers can therefore treat the first entry as a valid
/// default without a further existence check.
#[must_use]
pub fn available() -> Vec<ShellOption> {
    let mut shells = discover();
    if shells.is_empty() {
        shells.push(fallback_shell());
    }
    shells
}

/// Resolve the user's stored preference against what is installed.
///
/// An unknown or newly-uninstalled `preferred` id silently falls back to the
/// best available shell rather than erroring — the terminal opening with the
/// wrong shell is far better than it refusing to open at all.
#[must_use]
pub fn resolve(preferred: Option<&str>) -> ShellOption {
    let shells = available();
    preferred
        .and_then(|id| shells.iter().find(|s| s.id == id).cloned())
        .or_else(|| shells.into_iter().next())
        .unwrap_or_else(fallback_shell)
}

/// List the shells the embedded terminal can launch, best-first, for the
/// settings picker.
#[must_use]
pub fn list_shells() -> Vec<ShellOption> {
    available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_shells_all_exist_on_disk() {
        for shell in available() {
            // cmd.exe may be the bare-name fallback when COMSPEC is unset and
            // it isn't on PATH; every other entry was probed and must exist.
            if shell.path == "cmd.exe" {
                continue;
            }
            assert!(
                Path::new(&shell.path).is_file(),
                "offered shell {} does not exist at {}",
                shell.id,
                shell.path
            );
        }
    }

    #[test]
    fn shell_ids_are_unique() {
        let shells = available();
        let mut ids: Vec<_> = shells.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate shell ids in {shells:?}");
    }

    #[test]
    fn resolve_honours_a_known_preference() {
        let last = available().pop().expect("non-empty");
        assert_eq!(resolve(Some(&last.id)), last);
    }

    #[test]
    fn resolve_falls_back_when_the_preference_is_uninstalled() {
        assert_eq!(resolve(Some("definitely-not-a-shell")), available()[0]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_always_offers_a_command_prompt() {
        assert!(available().iter().any(|s| s.id == "cmd"));
    }

    #[cfg(windows)]
    #[test]
    fn git_bash_when_present_is_preferred_and_logs_in() {
        let shells = available();
        let Some(bash) = shells.iter().find(|s| s.id == "git-bash") else {
            return; // Git for Windows not installed on this machine.
        };
        assert_eq!(shells[0].id, "git-bash", "Git Bash should lead the list");
        assert!(bash.args.contains(&"--login".to_string()));
        assert!(bash.path.to_lowercase().ends_with("bin\\bash.exe"));
    }
}

//! `UniFFI` bridge exposing [`leogit_core`] to the native `SwiftUI` macOS client.
//!
//! This crate is glue only — it holds **no logic**. Every exported function is a
//! 1:1 delegation to `leogit-core`, mirroring what
//! `apps/tauri-app/src-tauri/src/shims/` does for the Tauri host. The two clients
//! therefore observe byte-identical behaviour; only the marshaling differs (JSON
//! over the `WebView` bridge vs. `UniFFI`'s binary encoding).
//!
//! # Type sharing
//!
//! Core's structs cross the boundary via `#[uniffi::remote]` declarations rather
//! than being redefined here. That keeps `leogit-core` free of any `UniFFI`
//! dependency (as it is free of Tauri) while still giving Swift the real types —
//! and because a remote declaration must restate the shape exactly, any drift in
//! core turns into a **compile error here** instead of a silent wire mismatch.
//!
//! # Threading
//!
//! Every function below is blocking: it shells out to `git` and waits. Swift must
//! call these off the main actor (see `Sources/LeoGit/IPC/`), exactly as the Tauri
//! host runs them on worker threads.

// Exported signatures are dictated by UniFFI, not by Rust ergonomics: arguments
// arrive as owned values across the FFI boundary, so taking `String` by value is
// required rather than avoidable.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use leogit_core::git;

// Re-exported so Swift sees the real core types. Names are used by the
// `#[uniffi::remote]` declarations below.
pub use leogit_core::git::{CommitInfo, FileEntry, FileStatus, LogOptions, RepoStatus};

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A failure reported by `leogit-core`.
///
/// Core models every fallible operation as `Result<T, String>` — a
/// human-readable message already suitable for display — so the bridge carries
/// that message across verbatim instead of inventing a taxonomy the core does
/// not have.
#[derive(Debug, uniffi::Error)]
pub enum GitError {
    /// The underlying git operation failed; `message` is core's own text.
    Failed {
        /// Human-readable failure text, ready to show to the user.
        message: String,
    },
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Failed { message } = self;
        f.write_str(message)
    }
}

impl std::error::Error for GitError {}

impl From<String> for GitError {
    fn from(message: String) -> Self {
        Self::Failed { message }
    }
}

// ---------------------------------------------------------------------------
// Shared types (defined in leogit-core, glued here)
// ---------------------------------------------------------------------------
//
// Each block below must mirror its core counterpart field-for-field; UniFFI
// generates lowering/lifting code that touches the fields directly, so a rename
// or type change in core fails this crate's build.

/// Mirrors [`leogit_core::git::FileStatus`].
#[uniffi::remote(Enum)]
pub enum FileStatus {
    New,
    Modified,
    Deleted,
    Renamed,
    Conflicted,
}

/// Mirrors [`leogit_core::git::FileEntry`].
#[uniffi::remote(Record)]
pub struct FileEntry {
    pub path: String,
    pub orig_path: Option<String>,
    pub status: FileStatus,
    pub xy: String,
    pub display_name: String,
    pub display_dir: String,
    pub embedded: bool,
    pub submodule_dirty: bool,
}

/// Mirrors [`leogit_core::git::RepoStatus`].
#[uniffi::remote(Record)]
pub struct RepoStatus {
    pub branch: String,
    pub upstream: String,
    pub has_upstream: bool,
    pub ahead: i32,
    pub behind: i32,
    pub files: Vec<FileEntry>,
    pub has_remote: bool,
    pub unpushed_shas: Vec<String>,
    pub detached: bool,
    pub head_sha: String,
}

/// Mirrors [`leogit_core::git::CommitInfo`].
#[uniffi::remote(Record)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub summary: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub committer_name: String,
    pub committer_date: String,
    pub parents: Vec<String>,
    pub trailers: Vec<String>,
    pub co_authors: Vec<String>,
    pub body_without_coauthors: String,
    pub tags: Vec<String>,
}

/// Mirrors [`leogit_core::git::LogOptions`].
#[uniffi::remote(Record)]
pub struct LogOptions {
    pub max_count: i32,
    pub skip: i32,
}

// ---------------------------------------------------------------------------
// Exported functions — open → status → log
// ---------------------------------------------------------------------------

/// Resolve `path` to the root of the repository containing it.
///
/// Accepts either a repository root or any subdirectory of one, matching the
/// `leogit <path>` CLI behaviour.
///
/// # Errors
///
/// Returns [`GitError`] when `path` is not inside a git repository.
#[uniffi::export]
pub fn resolve_repo_root(path: String) -> Result<String, GitError> {
    git::repo_root(Path::new(&path)).ok_or_else(|| GitError::Failed {
        message: format!("{path} is not a git repository"),
    })
}

/// The display name for the repository at `path` (its directory name).
#[must_use]
#[uniffi::export]
pub fn repo_display_name(path: String) -> String {
    git::get_repo_name(&path)
}

/// Working-tree status: branch metadata plus the list of changed files.
///
/// # Errors
///
/// Returns [`GitError`] when `git status` fails — for example if `repo_path` is
/// no longer a repository, or git is missing from `PATH`.
#[uniffi::export]
pub fn get_status(repo_path: String) -> Result<RepoStatus, GitError> {
    git::get_status(repo_path).map_err(GitError::from)
}

/// A page of commit history, newest first.
///
/// # Errors
///
/// Returns [`GitError`] when `git log` fails. A repository with no commits yet
/// is *not* an error — it yields an empty list.
#[uniffi::export]
pub fn get_log(repo_path: String, options: LogOptions) -> Result<Vec<CommitInfo>, GitError> {
    git::get_log(repo_path, options).map_err(GitError::from)
}

/// Version of the bridge, for smoke-testing that Swift is talking to the Rust
/// build it expects.
#[must_use]
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_repo_root_rejects_non_repo() {
        let err = resolve_repo_root("/".to_string()).unwrap_err();
        assert!(matches!(err, GitError::Failed { .. }));
    }

    #[test]
    fn resolve_repo_root_finds_this_repo_from_a_subdirectory() {
        // This crate's own directory is nested several levels inside the repo.
        let here = env!("CARGO_MANIFEST_DIR");
        let root = resolve_repo_root(here.to_string()).expect("leogit is a git repo");
        assert!(Path::new(&root).join(".git").exists());
    }

    #[test]
    fn core_version_is_reported() {
        assert!(!core_version().is_empty());
    }
}

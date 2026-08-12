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

use leogit_core::{diff, git, highlight};

// Re-exported so Swift sees the real core types. Names are used by the
// `#[uniffi::remote]` declarations below.
pub use leogit_core::diff::{DiffLine, FileDiff, Hunk, HunkHeader, IntraLineRange, LineType};
pub use leogit_core::git::{CommitInfo, FileEntry, FileStatus, LogOptions, RepoStatus};
pub use leogit_core::highlight::{BlobSource, Token, TokenClass};

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

/// Mirrors [`leogit_core::diff::LineType`].
#[uniffi::remote(Enum)]
pub enum LineType {
    Context,
    Add,
    Delete,
    Hunk,
    NoNewline,
}

/// Mirrors [`leogit_core::diff::IntraLineRange`].
#[uniffi::remote(Record)]
pub struct IntraLineRange {
    pub start: u32,
    pub length: u32,
}

/// Mirrors [`leogit_core::diff::DiffLine`].
#[uniffi::remote(Record)]
pub struct DiffLine {
    pub text: String,
    pub content: String,
    pub line_type: LineType,
    pub old_line_no: Option<i32>,
    pub new_line_no: Option<i32>,
    pub intra_line_diff: Option<IntraLineRange>,
}

/// Mirrors [`leogit_core::diff::HunkHeader`].
#[uniffi::remote(Record)]
pub struct HunkHeader {
    pub old_start: i32,
    pub old_count: i32,
    pub new_start: i32,
    pub new_count: i32,
}

/// Mirrors [`leogit_core::diff::Hunk`].
#[uniffi::remote(Record)]
pub struct Hunk {
    pub header: HunkHeader,
    pub lines: Vec<DiffLine>,
}

/// Mirrors [`leogit_core::diff::FileDiff`].
#[uniffi::remote(Record)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub file_header: String,
    pub hunks: Vec<Hunk>,
    pub is_binary: bool,
}

/// Mirrors [`leogit_core::highlight::TokenClass`].
#[uniffi::remote(Enum)]
pub enum TokenClass {
    Plain,
    Keyword,
    String,
    Comment,
    Function,
    Type,
    Variable,
    Number,
    Constant,
    Operator,
    Punctuation,
    Tag,
    Attribute,
    Builtin,
    Decorator,
    Heading,
    Strong,
    Emphasis,
    Strikethrough,
    Link,
    Raw,
    Quote,
}

/// Mirrors [`leogit_core::highlight::Token`].
#[uniffi::remote(Record)]
pub struct Token {
    pub start: u32,
    pub end: u32,
    pub class: TokenClass,
}

/// Mirrors [`leogit_core::highlight::BlobSource`].
#[uniffi::remote(Enum)]
pub enum BlobSource {
    WorkingTree { repo_path: String },
    Commit { repo_path: String, sha: String },
}

/// The structured result of parsing one file's raw diff.
///
/// A purpose-built record rather than a `#[uniffi::remote]` mirror of core's
/// [`diff::ParsedDiff`]: that struct also carries phase-1 HTML strings and
/// side-by-side row pairs — `WebView` presentation the native client neither
/// needs nor should pay to marshal. `file_diff` is still the real core type.
#[derive(uniffi::Record)]
pub struct DiffPayload {
    /// The parsed diff: hunks of typed lines plus file-level metadata.
    pub file_diff: FileDiff,
    /// Added-line total for the header badge (0 for binary diffs).
    pub additions: u32,
    /// Deleted-line total for the header badge (0 for binary diffs).
    pub deletions: u32,
}

impl From<diff::ParsedDiff> for DiffPayload {
    fn from(parsed: diff::ParsedDiff) -> Self {
        Self {
            file_diff: parsed.file_diff,
            additions: parsed.additions,
            deletions: parsed.deletions,
        }
    }
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

// ---------------------------------------------------------------------------
// Exported functions — diff pipeline
// ---------------------------------------------------------------------------
//
// Three steps, same as the Tauri client: raw text → structure → tokens. Kept
// separate (rather than fused into one call) so the UI can render the plain
// structured diff immediately and apply syntax colour when tokenization —
// which may read and parse whole blobs — catches up.

/// The raw unified diff for one working-tree file, `HEAD` against the working
/// tree (staged and unstaged combined). Untracked files diff against
/// `/dev/null`, so a brand-new file still yields hunks.
///
/// # Errors
///
/// Returns [`GitError`] when `git diff` fails.
#[uniffi::export]
pub fn get_diff(repo_path: String, file: FileEntry) -> Result<String, GitError> {
    git::get_diff(repo_path, file).map_err(GitError::from)
}

/// Parse a raw unified diff into hunks of typed lines.
///
/// Returns `None` for input that contains no parseable diff — an empty string,
/// or output for a file with no textual changes (e.g. a pure mode change).
#[must_use]
#[uniffi::export]
pub fn parse_diff(raw: String) -> Option<DiffPayload> {
    diff::parse_diff(raw).map(DiffPayload::from)
}

/// Syntax tokens for a parsed diff: one entry per flattened line of
/// `file_diff` (hunks concatenated, each hunk's `@@` header included), empty
/// where the tokenizer has nothing to say. Token `start`/`end` are code-point
/// indices into the line's `content`, the same unit as `IntraLineRange`.
///
/// `source` tells the tokenizer where to read complete blobs from, which keeps
/// multi-line constructs (block comments, embedded languages) correct;
/// without it, tokenization falls back to the diff's own lines.
#[must_use]
#[uniffi::export]
pub fn tokenize_diff(file_diff: FileDiff, source: Option<BlobSource>) -> Vec<Vec<Token>> {
    highlight::tokenize_diff(&file_diff, source.as_ref())
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

    /// Runs `git` in `dir`, panicking on failure so a broken fixture is loud.
    fn run_git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The full diff pipeline against a real throwaway repository: status →
    /// raw diff → parse → tokenize, for both a modified and an untracked file.
    /// Pins the parallel-array contract: tokenize returns exactly one entry
    /// per flattened `file_diff` line.
    #[test]
    fn diff_pipeline_parses_and_tokenizes_working_tree_changes() {
        let dir = std::env::temp_dir().join(format!("leogit-ffi-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        run_git(&dir, &["init"]);
        std::fs::write(
            dir.join("main.rs"),
            "fn main() {\n    println!(\"a\");\n}\n",
        )
        .expect("write");
        run_git(&dir, &["add", "."]);
        run_git(
            &dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-m",
                "init",
            ],
        );
        std::fs::write(
            dir.join("main.rs"),
            "fn main() {\n    println!(\"b\");\n}\n",
        )
        .expect("write");
        std::fs::write(dir.join("new.rs"), "pub fn added() {}\n").expect("write");

        let repo = dir.to_string_lossy().to_string();
        let status = get_status(repo.clone()).expect("status");
        assert_eq!(status.files.len(), 2, "one modified + one untracked");

        for file in status.files {
            let raw = get_diff(repo.clone(), file.clone()).expect("diff");
            let payload = parse_diff(raw).expect("parses");
            assert!(payload.additions > 0, "{} adds lines", file.path);

            let flat: usize = payload.file_diff.hunks.iter().map(|h| h.lines.len()).sum();
            let tokens = tokenize_diff(
                payload.file_diff,
                Some(BlobSource::WorkingTree {
                    repo_path: repo.clone(),
                }),
            );
            assert_eq!(
                tokens.len(),
                flat,
                "{}: one token line per diff line",
                file.path
            );
            assert!(
                tokens.iter().any(|line| !line.is_empty()),
                "{}: rust source produces at least one token",
                file.path
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

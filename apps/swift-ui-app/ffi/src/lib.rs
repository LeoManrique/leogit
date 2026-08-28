//! `UniFFI` bridge exposing [`leogit_core`] to the native `SwiftUI` macOS client.
//!
//! This crate is glue only — it holds **no logic**. Every exported function is a
//! 1:1 delegation to `leogit-core`, mirroring what
//! `apps/tauri-app/src-tauri/src/shims/` does for the Tauri host. The two clients
//! therefore observe byte-identical behaviour; only the marshaling differs (JSON
//! over the `WebView` bridge vs. `UniFFI`'s binary encoding). The one deliberate
//! exception: glue the Tauri client keeps in its TypeScript layer (the config →
//! [`AiProviderConfig`] mapping) lives here in Rust instead, still one
//! implementation per behaviour — just owned by the bridge rather than the UI.
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
//! Most functions below are blocking: they shell out to `git` and wait. Swift
//! must call these off the main actor (see `Sources/LeoGit/IPC/`), exactly as
//! the Tauri host runs them on worker threads. The network operations
//! (`fetch` / `pull` / `push` / `clone_repo` and the `gh` calls) are exported
//! async instead — core drives them through `spawn_blocking`, which needs a
//! live tokio context, hence `async_runtime = "tokio"` — and surface in Swift
//! as native `async` calls.
//! Progress callbacks arrive on core's stderr-reader thread, never the main
//! one; Swift listeners must hop to the main actor themselves.

// Exported signatures are dictated by UniFFI, not by Rust ergonomics: arguments
// arrive as owned values across the FFI boundary, so taking `String` by value is
// required rather than avoidable.
#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use leogit_core::events::{CoreEvent, EventSink};
use leogit_core::{
    ai, config, diff, exclusions, gh, git, highlight, launch, os, process, repos, shell, terminal,
    update,
};

// Re-exported so Swift sees the real core types. Names are used by the
// `#[uniffi::remote]` declarations below.
pub use leogit_core::ai::{AiProviderConfig, CommitMessage, ProviderStatus};
pub use leogit_core::config::{
    Bounds, ClaudeConfig, Config, ConfigBounds, ConfigPatch, OllamaConfig, ReposState,
    ReposStatePatch,
};
pub use leogit_core::diff::{
    DiffLine, DiffOptions, DiffSizeGuard, DiffSizeReason, EmptyDiffReason, FileDiff, Hunk,
    HunkHeader, IntraLineRange, LineType,
};
pub use leogit_core::events::TerminalExit;
pub use leogit_core::exclusions::Exclusion;
pub use leogit_core::gh::GhRepo;
pub use leogit_core::git::{
    BranchInfo, CommitDetail, CommitInfo, CommitStats, DiscardPlan, FileEntry, FileStatus,
    FileStatusStyle, LogOptions, MergeResult, RepoIdentifier, RepoStatus, RepoSync, SyncProposal,
};
pub use leogit_core::highlight::{BlobSource, Token, TokenClass};
pub use leogit_core::launch::LaunchTarget;
pub use leogit_core::repos::{CloneTarget, RepoRow};
pub use leogit_core::shell::ShellOption;
pub use leogit_core::terminal::StartedTerminal;
pub use leogit_core::update::UpdateInfo;

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
    pub stat_stamp: Option<String>,
}

/// Mirrors [`leogit_core::exclusions::Exclusion`].
#[uniffi::remote(Record)]
pub struct Exclusion {
    pub path: String,
    pub absent_ms: u32,
    pub absent_reads: u32,
}

/// Mirrors [`leogit_core::git::SyncProposal`].
#[uniffi::remote(Enum)]
pub enum SyncProposal {
    Loading,
    Detached,
    PublishRepository,
    PublishBranch,
    Pull,
    Push,
    Fetch,
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
    pub merging: bool,
    pub proposal: SyncProposal,
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

/// Mirrors [`leogit_core::git::CommitStats`].
#[uniffi::remote(Record)]
pub struct CommitStats {
    pub additions: u32,
    pub deletions: u32,
}

/// Mirrors [`leogit_core::git::BranchInfo`].
#[uniffi::remote(Record)]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
    pub is_current: bool,
}

/// Mirrors [`leogit_core::git::MergeResult`].
#[uniffi::remote(Record)]
pub struct MergeResult {
    pub success: bool,
    pub fast_forward: bool,
    pub conflicts: Vec<String>,
    pub error_message: Option<String>,
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
    pub text: Option<String>,
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

/// Mirrors [`leogit_core::git::FileStatusStyle`].
#[uniffi::remote(Record)]
pub struct FileStatusStyle {
    pub status: FileStatus,
    pub letter: String,
    pub label: String,
}

/// Mirrors [`leogit_core::git::CommitDetail`].
#[uniffi::remote(Record)]
pub struct CommitDetail {
    pub files: Vec<FileEntry>,
    pub stats: CommitStats,
}

/// Mirrors [`leogit_core::git::DiscardPlan`].
#[uniffi::remote(Record)]
pub struct DiscardPlan {
    pub restore: Vec<String>,
    pub trash: Vec<String>,
}

/// Mirrors [`leogit_core::diff::EmptyDiffReason`].
#[uniffi::remote(Enum)]
pub enum EmptyDiffReason {
    NoChanges,
    WhitespaceOnly,
    NoTextualChanges,
}

/// Mirrors [`leogit_core::diff::DiffSizeReason`].
#[uniffi::remote(Enum)]
pub enum DiffSizeReason {
    TotalBytes,
    LineLength,
}

/// Mirrors [`leogit_core::diff::DiffSizeGuard`].
#[uniffi::remote(Record)]
pub struct DiffSizeGuard {
    pub reason: DiffSizeReason,
    pub bytes: u64,
    pub longest_line: u64,
}

/// Mirrors [`leogit_core::diff::DiffOptions`].
///
/// The native client renders from the line model, so it asks for neither
/// `html` nor `side_by_side` — the fields exist because core serves a
/// `WebView` host too, and declaring them keeps this record honest about the
/// shared type rather than hiding the choice.
#[uniffi::remote(Record)]
pub struct DiffOptions {
    pub html: bool,
    pub side_by_side: bool,
    pub show_anyway: bool,
}

/// Mirrors [`leogit_core::repos::RepoRow`].
#[uniffi::remote(Record)]
pub struct RepoRow {
    pub path: String,
    pub names: Vec<String>,
}

/// Mirrors [`leogit_core::repos::CloneTarget`].
#[uniffi::remote(Record)]
pub struct CloneTarget {
    pub normalized_url: String,
    pub repo_name: String,
    pub target_path: String,
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

/// Mirrors [`leogit_core::launch::LaunchTarget`].
#[uniffi::remote(Record)]
pub struct LaunchTarget {
    pub path: String,
    pub is_repo: bool,
}

/// Mirrors [`leogit_core::update::UpdateInfo`].
#[uniffi::remote(Record)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub install_command: Option<String>,
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
    /// Set when there are no lines to show, and why — what the pane's empty
    /// state says instead of guessing at three unrelated causes at once.
    pub empty_reason: Option<EmptyDiffReason>,
    /// Set when the diff was withheld for its size. Ask again with
    /// `DiffOptions::show_anyway` to render it regardless.
    pub size_guard: Option<DiffSizeGuard>,
}

impl From<diff::ParsedDiff> for DiffPayload {
    fn from(parsed: diff::ParsedDiff) -> Self {
        Self {
            file_diff: parsed.file_diff,
            additions: parsed.additions,
            deletions: parsed.deletions,
            empty_reason: parsed.empty_reason,
            size_guard: parsed.size_guard,
        }
    }
}

// ---------------------------------------------------------------------------
// Exported functions — host bootstrap
// ---------------------------------------------------------------------------

/// Replace this process's `PATH` with the user's interactive login `PATH`,
/// so spawned tools (`git`, `gh`, the `claude` CLI) resolve when the app is
/// launched from Finder rather than a terminal. Must be the host's first
/// call, before any other thread can be reading the environment — the Swift
/// app runs it in `App.init`, exactly as the Tauri host runs it at the top
/// of `main`.
#[uniffi::export]
pub fn fix_path_env() {
    process::fix_path_env();
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
    git::resolve_repo_root(&path).map_err(|message| GitError::Failed { message })
}

/// `git init` a folder so it can be opened, returning the absolute path to
/// open. Backs the "this folder isn't a repository yet" prompt a
/// `leogit <dir>` invocation raises.
///
/// Idempotent: a folder already inside a repository yields that repository's
/// root rather than nesting a new one, so confirming twice — or confirming
/// after the user ran `git init` in a terminal — opens rather than fails.
///
/// # Errors
///
/// Returns [`GitError`] when the folder can't be created or resolved
/// (permissions, a file in the way), or when `git init` itself fails.
#[uniffi::export]
pub fn init_repo(path: String) -> Result<String, GitError> {
    git::init_repo(&path).map_err(GitError::from)
}

/// The display name for the repository at `path` (its directory name).
#[must_use]
#[uniffi::export]
pub fn repo_display_name(path: String) -> String {
    git::get_repo_name(&path)
}

/// The letter and name for every [`FileStatus`] — a table, fetched once, so
/// neither client writes its own set and they can't disagree again. Colour
/// stays per-platform.
#[must_use]
#[uniffi::export]
pub fn file_status_styles() -> Vec<FileStatusStyle> {
    git::file_status_styles()
}

/// Working-tree status: branch metadata, the list of changed files, and
/// whether a merge is in progress.
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
// Two steps: read-and-parse, then tokens. The read and the parse are one call
// because nothing ever wanted one without the other, and splitting them cost a
// round trip per file selection — and left the "empty because whitespace is
// hidden" answer with nowhere to be computed. Tokenization stays separate so
// the UI can render the structured diff immediately and apply syntax colour
// when the tokenizer — which may read and parse whole blobs — catches up.

/// Read and parse one working-tree file's diff: `HEAD` against the working
/// tree (staged and unstaged combined), untracked files against `/dev/null`
/// so a brand-new file still yields hunks.
///
/// `hide_whitespace` runs `git diff -w`; if that leaves nothing to show, core
/// checks the unfiltered diff so the pane can say the change *is* there and
/// the setting is hiding it.
///
/// # Errors
///
/// Returns [`GitError`] when `git diff` fails — which the caller must keep
/// distinct from an empty result, since a stale diff behind an error is the
/// one thing the pane must never show.
#[uniffi::export]
pub fn get_parsed_diff(
    repo_path: String,
    file: FileEntry,
    hide_whitespace: bool,
    options: DiffOptions,
) -> Result<DiffPayload, GitError> {
    diff::get_parsed_diff(repo_path, file, hide_whitespace, options)
        .map(DiffPayload::from)
        .map_err(GitError::from)
}

/// Read and parse one file's diff within a commit (that commit against its
/// first parent). An empty `file_path` yields the whole-commit diff.
///
/// # Errors
///
/// Returns [`GitError`] when `git log` fails.
#[uniffi::export]
pub fn get_parsed_commit_diff(
    repo_path: String,
    sha: String,
    file_path: String,
    options: DiffOptions,
) -> Result<DiffPayload, GitError> {
    diff::get_parsed_commit_diff(repo_path, sha, file_path, options)
        .map(DiffPayload::from)
        .map_err(GitError::from)
}

/// Plain text of a flat line range, for the clipboard — rebuilt from the line
/// model so a copy carries the file's own lines rather than whatever the view
/// drew around them (gutters, `+`/`−` prefixes, expanded tabs).
#[must_use]
#[uniffi::export]
pub fn copy_diff_text(file_diff: FileDiff, start: u32, end: u32) -> String {
    diff::copy_text(&file_diff, start as usize, end as usize)
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

// ---------------------------------------------------------------------------
// Exported functions — commit history detail
// ---------------------------------------------------------------------------
//
// The History detail pane's two reads: commit metadata rides in the
// `CommitInfo` the list already holds, so selecting a commit fetches its
// changed files and +/− totals together, then — per selected file — a parsed
// diff from the same pipeline as the working tree (with `BlobSource::Commit`
// so blobs are read at the commit, not from disk). Both use `--first-parent`
// in core, so a merge commit shows its first-parent changes rather than
// `diff-tree`'s empty output.

/// The files a commit changed and its line totals, from one `git log`.
/// Renames carry `orig_path`; `embedded`/`submodule_dirty` are always false
/// here (they are working-tree concepts), and binary files contribute to the
/// file list but not to the totals.
///
/// # Errors
///
/// Returns [`GitError`] when `git log` fails — an unknown sha, most likely.
#[uniffi::export]
pub fn get_commit_detail(repo_path: String, sha: String) -> Result<CommitDetail, GitError> {
    git::get_commit_detail(repo_path, sha).map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — commit
// ---------------------------------------------------------------------------
//
// Two calls, same as the Tauri client: format the message, then commit the
// checked files. Core's `commit` owns the whole staging story — it resets the
// index and re-stages exactly `files` (whole-file `git add`/`update-index`),
// so there is no separate stage step to expose.

/// Join a summary, optional description, and `Co-authored-by` trailers into
/// the message `commit` expects.
#[must_use]
#[uniffi::export]
pub fn format_commit_message(
    summary: String,
    description: String,
    co_authors: Vec<String>,
) -> String {
    git::format_commit_message(summary, description, co_authors)
}

/// Commit exactly `files` with `message`, regardless of prior index state:
/// core resets the index, stages the given files (handling renamed and
/// deleted entries via `update-index`), and runs `git commit`. With
/// `amend = true` an empty file list is allowed (message-only amend).
///
/// # Errors
///
/// Returns [`GitError`] when no files are selected (non-amend), staging
/// produces no changes, or any underlying git command fails.
#[uniffi::export]
pub fn commit(
    repo_path: String,
    message: String,
    files: Vec<FileEntry>,
    amend: Option<bool>,
) -> Result<(), GitError> {
    git::commit(repo_path, message, files, amend).map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — row context actions (discard / ignore / reveal / open,
// checkout commit, undo commit)
// ---------------------------------------------------------------------------
//
// What the Tauri client hangs off a right-clicked file row or commit row. All
// of them already shipped there as registered commands; they simply had no
// bridge export until the native client grew the same menus. Sync, like every
// other git call here: each is a short-lived local operation, so the Swift
// wrappers' `@concurrent` hop is enough to keep them off the main actor —
// `spawn_blocking` stays reserved for the calls that block for many seconds by
// design (`repo_sync_status`, `discover_repos`).
//
// `os::open_url` is deliberately NOT exported: SwiftUI opens URLs itself
// (`Link` / `openURL`), so the native client has no caller for it.

/// What [`discard_files`] would do to each of `files`, path by path — the
/// confirmation dialog's copy, decided by the same code that performs the
/// action rather than inferred from a status letter (which gets a staged
/// re-add, an odd rename and an unborn HEAD wrong).
#[must_use]
#[uniffi::export]
pub fn classify_discard(repo_path: String, files: Vec<FileEntry>) -> DiscardPlan {
    git::classify_discard(&repo_path, &files)
}

/// Throw away the working-tree changes to `files`, restoring each to its
/// committed state.
///
/// Tracked files (modified, deleted, conflicted, and a rename's original side)
/// are restored from `HEAD` in both index and working tree. Files with no
/// committed version — untracked entries and a rename's new side — have no
/// state to restore, so their working-tree copy goes to the OS trash, which is
/// why the UI must confirm first. An empty list is a no-op.
///
/// # Errors
///
/// Returns [`GitError`] when the underlying `git reset`/`git checkout` fails. A
/// file that can't be trashed is skipped, not fatal.
#[uniffi::export]
pub fn discard_files(repo_path: String, files: Vec<FileEntry>) -> Result<(), GitError> {
    git::discard_files(&repo_path, files).map_err(GitError::from)
}

/// Add literal file paths to the repository's root `.gitignore`, escaping each
/// path's glob metacharacters so the rule matches that file and nothing else.
/// Rules already present are skipped, so repeated use can't pile up duplicates.
///
/// # Errors
///
/// Returns [`GitError`] when `.gitignore` can't be written.
#[uniffi::export]
pub fn ignore_paths(repo_path: String, paths: Vec<String>) -> Result<(), GitError> {
    git::ignore_paths(&repo_path, paths).map_err(GitError::from)
}

/// Append ready-to-write patterns (globs like `*.log`) to the repository's root
/// `.gitignore`. Literal paths belong in [`ignore_paths`], which escapes them
/// first.
///
/// # Errors
///
/// Returns [`GitError`] when `.gitignore` can't be written.
#[uniffi::export]
pub fn append_to_gitignore(repo_path: String, patterns: Vec<String>) -> Result<(), GitError> {
    git::append_to_gitignore(&repo_path, patterns).map_err(GitError::from)
}

/// Reveal `rel_path` (relative to `repo_path`) in the platform file manager,
/// selecting the item — Finder's `open -R` on macOS.
///
/// # Errors
///
/// Returns [`GitError`] when the file manager can't be spawned or doesn't
/// return within core's 15 s hand-off timeout.
#[uniffi::export]
pub fn reveal_path(repo_path: String, rel_path: String) -> Result<(), GitError> {
    os::reveal_path(repo_path, rel_path).map_err(GitError::from)
}

/// Open `rel_path` (relative to `repo_path`) with the OS's default application
/// for that file type.
///
/// # Errors
///
/// Returns [`GitError`] when the handler can't be spawned or doesn't return
/// within core's 15 s hand-off timeout.
#[uniffi::export]
pub fn open_path(repo_path: String, rel_path: String) -> Result<(), GitError> {
    os::open_path(repo_path, rel_path).map_err(GitError::from)
}

/// Open an `https://` URL in the default browser — the update chip's release
/// page.
///
/// Routed through core rather than `NSWorkspace` so both clients open a URL
/// behind the same scheme allowlist and metacharacter rejection: the address
/// comes from GitHub's own release payload, and a client that skipped the
/// guard would be the one place an unexpected one reached a shell.
///
/// # Errors
///
/// Returns [`GitError`] for a non-`https` URL or one containing shell
/// metacharacters, or when the browser can't be spawned.
#[uniffi::export]
pub fn open_url(url: String) -> Result<(), GitError> {
    os::open_url(url).map_err(GitError::from)
}

/// Check out a commit by sha, detaching `HEAD`. `get_status` then reports
/// `detached = true`; the branch picker is how the user reattaches.
///
/// # Errors
///
/// Returns [`GitError`] when git refuses — most commonly because uncommitted
/// changes would be overwritten. Git's message is surfaced verbatim.
#[uniffi::export]
pub fn checkout_commit(repo_path: String, sha: String) -> Result<(), GitError> {
    git::checkout_commit(&repo_path, &sha).map_err(GitError::from)
}

/// Undo the last commit: `git reset --mixed HEAD~1`. The commit is removed,
/// the index matches the new `HEAD`, and its changes re-appear in the working
/// tree as ordinary unstaged edits, ready to be re-committed.
///
/// # Errors
///
/// Returns [`GitError`] when `HEAD` has no parent (the initial commit, which
/// core refuses to undo) or the reset fails.
#[uniffi::export]
pub fn undo_last_commit(repo_path: String) -> Result<(), GitError> {
    git::undo_last_commit(repo_path).map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — branches
// ---------------------------------------------------------------------------

/// Local and remote branches in one flat list, most recent commit first.
/// Remote entries use their short form (`origin/feature`); each remote's HEAD
/// symref is skipped, so no phantom `origin` row appears.
///
/// # Errors
///
/// Returns [`GitError`] when `git for-each-ref` fails.
#[uniffi::export]
pub fn list_branches(repo_path: String) -> Result<Vec<BranchInfo>, GitError> {
    git::list_branches(repo_path).map_err(GitError::from)
}

/// Create a branch named `name` without checking it out. An empty
/// `start_point` branches from `HEAD` — the only form the clients use; both
/// chain `switch_branch` right after, so "New branch" lands the user on it.
///
/// # Errors
///
/// Returns [`GitError`] when git refuses — most commonly a name that already
/// exists or is not a valid ref name.
#[uniffi::export]
pub fn create_branch(repo_path: String, name: String, start_point: String) -> Result<(), GitError> {
    git::create_branch(repo_path, name, start_point).map_err(GitError::from)
}

/// Check out `branch`. A remote-only name (`origin/feature`) becomes a local
/// tracking branch, matching `git switch`'s DWIM, instead of detaching HEAD.
/// A dirty working tree is git's call: compatible changes travel across, and
/// git's own refusal is surfaced verbatim otherwise.
///
/// # Errors
///
/// Returns [`GitError`] when the checkout fails — most commonly because local
/// changes would be overwritten.
#[uniffi::export]
pub fn switch_branch(repo_path: String, branch: String) -> Result<(), GitError> {
    git::switch_branch(repo_path, branch).map_err(GitError::from)
}

/// Delete the local branch `name` — `git branch -D`, always forced, so an
/// unmerged branch is destroyed without a second look. The confirmation
/// belongs to the UI.
///
/// # Errors
///
/// Returns [`GitError`] when git refuses, e.g. deleting the checked-out
/// branch.
#[uniffi::export]
pub fn delete_branch(repo_path: String, name: String) -> Result<(), GitError> {
    git::delete_branch(repo_path, name).map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — merge
// ---------------------------------------------------------------------------
//
// A failed merge is data, not an error: `MergeResult.success == false` with
// git's text in `error_message` and the conflicted paths in `conflicts`. The
// `Result` only turns `Err` when git can't run at all. Squash is two calls —
// `merge_squash`, then `commit_squash_merge` once it succeeds — the exact
// sequence the Tauri handlers use.

/// `git merge --no-edit <branch>` into the current branch. On success,
/// `fast_forward` reports whether git fast-forwarded instead of creating a
/// merge commit.
///
/// # Errors
///
/// Returns [`GitError`] only when the git process can't be spawned.
#[uniffi::export]
pub fn merge_branch(repo_path: String, branch: String) -> Result<MergeResult, GitError> {
    git::merge_branch(repo_path, branch).map_err(GitError::from)
}

/// `git merge --squash <branch>`: stages the combined changes but does not
/// commit them — call `commit_squash_merge` next. `fast_forward` is always
/// `false` here.
///
/// # Errors
///
/// Returns [`GitError`] only when the git process can't be spawned.
#[uniffi::export]
pub fn merge_squash(repo_path: String, branch: String) -> Result<MergeResult, GitError> {
    git::merge_squash(repo_path, branch).map_err(GitError::from)
}

/// Commit a successful squash merge with git's auto-generated message
/// ("Squashed commit of the following: …").
///
/// # Errors
///
/// Returns [`GitError`] when the commit fails.
#[uniffi::export]
pub fn commit_squash_merge(repo_path: String) -> Result<(), GitError> {
    git::commit_squash_merge(repo_path).map_err(GitError::from)
}

/// Abort an in-progress merge (`git merge --abort`), restoring the pre-merge
/// working tree.
///
/// # Errors
///
/// Returns [`GitError`] when there is no merge to abort or the abort fails.
#[uniffi::export]
pub fn merge_abort(repo_path: String) -> Result<(), GitError> {
    git::merge_abort(repo_path).map_err(GitError::from)
}

// `is_merging` stays unexported (the dead-surface rule): `RepoStatus.merging`
// carries the same answer on every refresh, which is where the UI reads it —
// and a separate call is what let one refresh path forget to ask.

/// How many commits merging `target_branch` would bring into the current
/// branch — the merge dialog's preview number.
///
/// # Errors
///
/// Returns [`GitError`] when the branches share no merge base or either ref
/// is unknown.
#[uniffi::export]
pub fn count_commits_to_merge(repo_path: String, target_branch: String) -> Result<i32, GitError> {
    git::count_commits_to_merge(repo_path, target_branch).map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — sync (fetch / pull / push / clone)
// ---------------------------------------------------------------------------
//
// The first async exports and the first foreign callback interface in the
// bridge. Core streams network progress through its `EventSink` seam;
// `ProgressSink` adapts that seam to a Swift-implemented
// `SyncProgressListener`, translating core's event into the flat
// `SyncProgress` record. There is no completion event — the operation's end
// is the awaited return value — so the UI drives its busy state off the
// `await`, exactly as the Tauri client wraps the invoke in try/finally.

/// One git network-progress tick.
///
/// A purpose-built record rather than a `#[uniffi::remote]` mirror of core's
/// [`leogit_core::events::GitProgress`]: that struct's `op` label is a
/// `&'static str`, which cannot cross the FFI, and `op`/`path` exist so
/// listeners on a process-wide sink can filter — here each listener is scoped
/// to a single operation, so only what the UI renders crosses.
#[derive(uniffi::Record)]
pub struct SyncProgress {
    /// Aggregate progress across the operation's phases, 0–100. Monotonically
    /// non-decreasing within one operation; context lines repeat the last
    /// value with fresh `text`.
    pub percent: f32,
    /// Raw git progress line, shown verbatim — e.g.
    /// `"Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s"`.
    pub text: String,
}

/// Swift-implemented receiver for git network progress.
///
/// Calls arrive on core's stderr-reader thread — never the main one — so
/// implementations must be thread-safe and must not block: core's contract
/// for `EventSink::emit` is that a dropped tick degrades to a stalled bar,
/// never a stalled operation.
#[uniffi::export(foreign)]
pub trait SyncProgressListener: Send + Sync {
    /// Deliver one progress tick.
    fn on_progress(&self, progress: SyncProgress);
}

/// Adapts core's [`EventSink`] to a [`SyncProgressListener`]. Non-git-progress
/// events (the terminal variants) are ignored: this sink is only ever handed
/// to `pull`/`push`/`clone_repo`/`gh_clone`, which emit nothing else.
struct ProgressSink {
    listener: Arc<dyn SyncProgressListener>,
}

impl EventSink for ProgressSink {
    fn emit(&self, event: CoreEvent) {
        if let CoreEvent::GitProgress(progress) = event {
            self.listener.on_progress(SyncProgress {
                percent: progress.percent,
                text: progress.text,
            });
        }
    }
}

/// The name of the repository's first remote, falling back to the literal
/// `"origin"` when none is configured. Both clients resolve this immediately
/// before each network operation rather than caching it.
///
/// # Errors
///
/// Returns [`GitError`] when `git remote` itself fails.
#[uniffi::export]
pub fn get_remote(repo_path: String) -> Result<Option<String>, GitError> {
    git::get_remote(repo_path).map_err(GitError::from)
}

/// `git fetch --prune <remote>`: refresh remote-tracking refs — and the
/// ahead/behind counts derived from them — without touching the working
/// tree. Fetch does not stream progress; core's fetch path has no sink.
///
/// # Errors
///
/// Returns [`GitError`] when the fetch fails — unreachable remote, timeout.
#[uniffi::export(async_runtime = "tokio")]
pub async fn fetch(repo_path: String, remote: String, background: bool) -> Result<(), GitError> {
    git::fetch(repo_path, remote, background)
        .await
        .map_err(GitError::from)
}

/// `git pull --ff --progress <remote>`. Fast-forward only: a diverged branch
/// fails with git's own message instead of creating a merge or rebase.
///
/// # Errors
///
/// Returns [`GitError`] when the pull fails — diverged branch, unreachable
/// remote, or local changes that would be overwritten.
#[uniffi::export(async_runtime = "tokio")]
pub async fn pull(
    repo_path: String,
    remote: String,
    listener: Arc<dyn SyncProgressListener>,
) -> Result<(), GitError> {
    git::pull(Arc::new(ProgressSink { listener }), repo_path, remote)
        .await
        .map_err(GitError::from)
}

/// `git push --progress [--set-upstream] [--force-with-lease] <remote>
/// <branch>`.
///
/// `set_upstream` must be derived from `RepoStatus.has_upstream` (pass
/// `!has_upstream`), never synthesised: that flag is only true when real
/// tracking configuration exists, and dropping `--set-upstream` on a first
/// push leaves the branch permanently untracked. `force_with_lease` is the
/// only force mode core offers — there is no bare `--force` path.
///
/// # Errors
///
/// Returns [`GitError`] when the push is rejected (non-fast-forward, stale
/// lease) or the remote is unreachable.
#[uniffi::export(async_runtime = "tokio")]
pub async fn push(
    repo_path: String,
    remote: String,
    branch: String,
    set_upstream: bool,
    force_with_lease: bool,
    listener: Arc<dyn SyncProgressListener>,
) -> Result<(), GitError> {
    git::push(
        Arc::new(ProgressSink { listener }),
        repo_path,
        remote,
        branch,
        set_upstream,
        force_with_lease,
    )
    .await
    .map_err(GitError::from)
}

/// `git clone --progress <url> <target_path>` — the Clone flow's URL path.
/// `target_path` is the full destination (parent plus repo folder), not a
/// parent directory: deriving the folder name from the URL is the UI's job,
/// exactly as in the Tauri dialog. Core expands a leading `~`, refuses an
/// existing path, and creates the parent; git creates the leaf. Returns the
/// absolute path of the fresh clone, ready to open. Progress streams through
/// the same listener seam as pull/push — clone's phase weights are applied
/// inside core, so `percent` arrives aggregated here like every other op.
///
/// # Errors
///
/// Returns [`GitError`] with git's own combined output — unreachable URL,
/// missing credentials (interactive prompts are hard-disabled, so private
/// remotes fail instead of hanging), or an existing destination.
#[uniffi::export(async_runtime = "tokio")]
pub async fn clone_repo(
    url: String,
    target_path: String,
    listener: Arc<dyn SyncProgressListener>,
) -> Result<String, GitError> {
    git::clone_repo(Arc::new(ProgressSink { listener }), url, target_path)
        .await
        .map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — GitHub CLI (clone, publish)
// ---------------------------------------------------------------------------
//
// Everything here shells out to `gh` so its stored auth is ambient — private
// repos work without a prompt. None of it streams progress: `gh` reports
// nothing parseable, so the UIs show indeterminate bars for the transfers
// (clone, publish). `check_auth` stays unexported (the
// dead-surface rule): neither client reads it — every gh call's own error
// text already distinguishes "gh missing" from "not authenticated".

/// Mirrors [`leogit_core::gh::GhRepo`] — one row of the Clone dialog's
/// GitHub tab. `pushed_at` is an ISO-8601 last-push timestamp; the "recent"
/// sort compares it lexically, exactly like the Tauri client.
#[uniffi::remote(Record)]
pub struct GhRepo {
    pub name_with_owner: String,
    pub name: String,
    pub description: String,
    pub is_private: bool,
    pub pushed_at: String,
}

/// The signed-in user's GitHub repositories, most recently pushed first —
/// forks included, archived skipped. Async over `spawn_blocking` like
/// [`repo_sync_status`]: the `gh` query can hold its thread for its full
/// 20 s timeout, which must never be a Swift cooperative thread.
///
/// # Errors
///
/// Returns [`GitError`] with a dialog-ready message: `gh` not installed,
/// not authenticated, or timed out.
#[uniffi::export(async_runtime = "tokio")]
pub async fn gh_repo_list(limit: u32) -> Result<Vec<GhRepo>, GitError> {
    tokio::task::spawn_blocking(move || gh::gh_repo_list(limit))
        .await
        .map_err(|join_error| GitError::Failed {
            message: format!("gh repo list did not complete: {join_error}"),
        })?
        .map_err(GitError::from)
}

/// `gh repo clone <owner/name> <target_path>` — clone through the GitHub
/// CLI so its auth covers private repositories. Same destination contract
/// as [`clone_repo`] (full path, `~` expanded, existing path refused,
/// parent created); returns the absolute path of the fresh clone.
///
/// # Errors
///
/// Returns [`GitError`] when `gh` is missing, the clone fails or times out
/// (600 s cap), or the destination already exists.
#[uniffi::export(async_runtime = "tokio")]
pub async fn gh_clone(
    listener: Arc<dyn SyncProgressListener>,
    name_with_owner: String,
    target_path: String,
) -> Result<String, GitError> {
    gh::gh_clone(
        Arc::new(ProgressSink { listener }),
        name_with_owner,
        target_path,
    )
    .await
    .map_err(GitError::from)
}

/// `gh repo create <name> --source <repo_path> --remote origin --push` — the
/// one-shot "Publish Repository" flow for a remote-less repo: creates the
/// GitHub repository under the signed-in account (`name` may be
/// `owner/name` to target an organisation), wires it up as `origin`, and
/// pushes the current branch with tracking. The next status refresh sees
/// `has_remote`/`has_upstream` flip, which is how the UI learns it worked.
///
/// # Errors
///
/// Returns [`GitError`] when the trimmed name is empty, `gh` is missing or
/// the operation times out (600 s cap), or `gh repo create` fails — auth,
/// name collision, push rejection — with gh's own stderr verbatim.
#[uniffi::export(async_runtime = "tokio")]
pub async fn gh_publish_repo(
    repo_path: String,
    name: String,
    description: String,
    is_private: bool,
) -> Result<(), GitError> {
    gh::gh_publish_repo(repo_path, name, description, is_private)
        .await
        .map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — AI commit message
// ---------------------------------------------------------------------------
//
// The same two-step pipeline as the Tauri composer: gather the checked files'
// combined diff (`get_selected_diff`), then hand that one string to the
// provider (`generate_commit_message`). Which provider — and its model and
// endpoint — comes from the shared `~/.config/leogit/config.toml`, so both
// clients honour one setting. The Tauri client maps that config into an
// `AiProviderConfig` in TypeScript before every call; `load_ai_config` is
// that same mapping owned here in Rust instead, where core drift becomes a
// compile error rather than a silent field mismatch.
//
// Readiness is two exports, not one, because the two provider states a user can
// fix are visible in different places: a signed-out CLI to a probe, an expired
// session only to a request that failed. Both clients ask both questions.

/// Mirrors [`leogit_core::ai::CommitMessage`] — the provider's already-split
/// suggestion: `title` fills the summary field, `description` the body.
#[uniffi::remote(Record)]
pub struct CommitMessage {
    pub title: String,
    pub description: String,
}

/// Mirrors [`leogit_core::ai::AiProviderConfig`].
///
/// `provider` is `"claude"` or `"ollama"`; core dispatches on the standalone
/// `provider` argument of [`generate_commit_message`] and treats this record
/// as provider knobs only. `None` fields fall back inside core: model
/// `"sonnet"` (claude) / `"tavernari/git-commit-message:latest"` (ollama),
/// base URL `http://localhost:11434`.
#[uniffi::remote(Record)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub timeout_secs: u32,
}

/// Mirrors [`leogit_core::ai::ProviderStatus`] — whether the provider can
/// serve a request, and when it can't, the sentence to show and the shell
/// command that would fix it (empty when there is none to offer).
#[uniffi::remote(Record)]
pub struct ProviderStatus {
    pub ready: bool,
    pub reason: String,
    pub fix_command: String,
}

/// The AI settings from the shared config file, ready to pass to
/// [`generate_commit_message`]. Loaded fresh before every generate — never
/// cached — so an edit to `config.toml` (or a save from the Tauri client)
/// takes effect on the next click.
///
/// The config→provider mapping itself lives in core, so the two clients
/// cannot drift on which setting feeds which knob; this is a delegation like
/// every other export here.
///
/// # Errors
///
/// Returns [`GitError`] when the config file exists but cannot be read or
/// parsed. A missing file is not an error — defaults are written and used.
#[uniffi::export]
pub fn load_ai_config() -> Result<AiProviderConfig, GitError> {
    ai::load_ai_config().map_err(GitError::from)
}

/// The combined unified diff of exactly `files` — each file diffed
/// individually (untracked files against `/dev/null`) and concatenated, the
/// string `generate_commit_message` takes as its input. An empty selection
/// yields an empty string, which generate then rejects as "no files
/// selected".
///
/// # Errors
///
/// Returns [`GitError`] when `git diff` itself fails; a file with no textual
/// changes simply contributes nothing.
#[uniffi::export]
pub fn get_selected_diff(repo_path: String, files: Vec<FileEntry>) -> Result<String, GitError> {
    git::get_selected_diff(repo_path, files).map_err(GitError::from)
}

/// Generate a commit message from `diff` via the chosen provider: the local
/// `claude` CLI (spawned from `PATH` — hence [`fix_path_env`]) or a
/// self-hosted Ollama instance. Plain request/response with a 120 s timeout
/// inside core; there is no streaming and no cancel, matching the Tauri
/// client. Async over tokio machinery (`tokio::process`, reqwest), hence
/// `async_runtime = "tokio"` like the sync exports above.
///
/// # Errors
///
/// Returns [`GitError`] with core's own text: empty diff, unknown provider,
/// oversized diff, spawn/timeout/HTTP failures, or a response no commit
/// message could be extracted from. Provider API errors surface as errors,
/// never as the message — pinned by core's envelope tests.
#[uniffi::export(async_runtime = "tokio")]
pub async fn generate_commit_message(
    diff: String,
    provider: String,
    config: AiProviderConfig,
) -> Result<CommitMessage, GitError> {
    ai::generate_commit_message(diff, provider, config)
        .await
        .map_err(GitError::from)
}

/// Ask whether `provider` could serve a request right now, so the composer can
/// say *why* Generate is greyed out instead of letting a doomed request report
/// it. Two questions for Claude (`--version`, then `auth status`), a request to
/// its own configured address for Ollama.
///
/// Every probe failure is an answer rather than an error, and an answer that
/// can't be interpreted reports ready: locking the user out of Generate because
/// a CLI changed its output format is worse than letting the request speak.
///
/// # Errors
///
/// Returns [`GitError`] only for a provider name core doesn't know.
#[uniffi::export(async_runtime = "tokio")]
pub async fn check_provider_status(
    provider: String,
    config: AiProviderConfig,
) -> Result<ProviderStatus, GitError> {
    ai::check_provider_status(provider, config)
        .await
        .map_err(GitError::from)
}

/// Read a *failed* generate for a provider state the user can fix.
///
/// Not a fallback for [`check_provider_status`] — for an expired session it is
/// the only thing that works, because signing out deletes the credentials (a
/// probe sees that) while an expired session leaves them on disk, so `claude
/// auth status` still reports a signed-in CLI and only a real request discovers
/// the refresh failed.
///
/// Reports ready for anything it doesn't recognize, so a caller can only ever
/// *raise* a remedy from it, never clear one.
#[uniffi::export]
#[must_use]
pub fn provider_status_from_failure(provider: String, error: String) -> ProviderStatus {
    ai::provider_status_from_failure(&provider, &error)
}

// ---------------------------------------------------------------------------
// Exported functions — repo directory & background refresh
// ---------------------------------------------------------------------------
//
// Everything the repo switcher and the background schedulers consume: the
// shared config file (auto-fetch cadence, scan roots), the shared repos-state
// file (last-opened repo + most-recently-used list), filesystem discovery,
// and the per-repo sync summary behind the picker's dirty/behind/ahead
// badges. Both files are the same ones the Tauri client reads and writes, so
// opening a repo in one client is what the other restores on launch.
//
// `get_ahead_behind` stays unexported (the dead-surface rule): `get_status`
// and `repo_sync_status` already carry the counts everywhere the UI needs
// them.

/// Mirrors [`leogit_core::config::Config`] — the shared `config.toml`, read
/// whole. The native client consumes `auto_fetch` / `fetch_interval_ms` (the
/// auto-fetch loop) and `scan_paths` / `scan_depth` (repo discovery); the
/// rest crosses so the record can't drift from core.
#[uniffi::remote(Record)]
pub struct Config {
    pub theme: String,
    pub fetch_interval_ms: u32,
    pub ai_provider: String,
    pub auto_fetch: bool,
    pub syntax_highlighting: bool,
    pub scan_paths: Vec<String>,
    pub scan_depth: u32,
    pub side_by_side_diff: bool,
    pub hide_whitespace: bool,
    pub tab_size: u32,
    pub terminal_shell: Option<String>,
    pub claude: ClaudeConfig,
    pub ollama: OllamaConfig,
}

/// Mirrors [`leogit_core::config::Bounds`].
#[uniffi::remote(Record)]
pub struct Bounds {
    pub min: u32,
    pub max: u32,
    pub fallback: u32,
}

/// Mirrors [`leogit_core::config::ConfigBounds`].
#[uniffi::remote(Record)]
pub struct ConfigBounds {
    pub fetch_interval_ms: Bounds,
    pub scan_depth: Bounds,
    pub tab_size: Bounds,
    pub ai_timeout_secs: Bounds,
}

/// Mirrors [`leogit_core::config::ClaudeConfig`].
#[uniffi::remote(Record)]
pub struct ClaudeConfig {
    pub model: Option<String>,
    pub timeout_secs: u32,
}

/// Mirrors [`leogit_core::config::OllamaConfig`].
#[uniffi::remote(Record)]
pub struct OllamaConfig {
    pub model: Option<String>,
    pub server_url: String,
    pub timeout_secs: u32,
}

/// Mirrors [`leogit_core::config::ConfigPatch`] — the only writer.
///
/// `None` leaves a field as it is on disk, so a surface patches what it owns
/// and nothing else: two clients share this file, and a whole-object write
/// reverted whatever the other had changed since the window opened. Clearing
/// an optional field is patching it to `""` (the config's standing
/// blank-means-absent rule), which is why these stay a single `Option` layer.
/// Every field defaults to "leave it alone", so a caller writes only what it
/// means to change — the whole point of a patch, and what makes the
/// one-field writes (a provider picker, a sort toggle) read as one line.
#[uniffi::remote(Record)]
pub struct ConfigPatch {
    #[uniffi(default = None)]
    pub theme: Option<String>,
    #[uniffi(default = None)]
    pub fetch_interval_ms: Option<u32>,
    #[uniffi(default = None)]
    pub ai_provider: Option<String>,
    #[uniffi(default = None)]
    pub auto_fetch: Option<bool>,
    #[uniffi(default = None)]
    pub syntax_highlighting: Option<bool>,
    #[uniffi(default = None)]
    pub scan_paths: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub scan_depth: Option<u32>,
    #[uniffi(default = None)]
    pub side_by_side_diff: Option<bool>,
    #[uniffi(default = None)]
    pub hide_whitespace: Option<bool>,
    #[uniffi(default = None)]
    pub tab_size: Option<u32>,
    #[uniffi(default = None)]
    pub terminal_shell: Option<String>,
    #[uniffi(default = None)]
    pub claude_model: Option<String>,
    #[uniffi(default = None)]
    pub claude_timeout_secs: Option<u32>,
    #[uniffi(default = None)]
    pub ollama_model: Option<String>,
    #[uniffi(default = None)]
    pub ollama_server_url: Option<String>,
    #[uniffi(default = None)]
    pub ollama_timeout_secs: Option<u32>,
}

/// Mirrors [`leogit_core::config::ReposState`] — the shared
/// `repos-state.json`: which repo to restore on launch, plus the
/// most-recently-opened list that drives the picker's tiered background
/// refresh.
#[uniffi::remote(Record)]
pub struct ReposState {
    pub last_opened_repo: Option<String>,
    pub last_clone_dir: Option<String>,
    pub repo_sort_mode: Option<String>,
    pub clone_sort_mode: Option<String>,
    pub recent_repos: Option<Vec<String>>,
}

/// Mirrors [`leogit_core::config::ReposStatePatch`]. `None` leaves a field
/// as it is on disk; `recent_repos` is deliberately absent from the patch —
/// the MRU list's only writer is [`record_recent_repo`].
///
/// Every field defaults to "leave it alone", like [`ConfigPatch`]: a writer
/// names the one field it owns and cannot silently carry the others'
/// stale values along with it.
#[uniffi::remote(Record)]
pub struct ReposStatePatch {
    #[uniffi(default = None)]
    pub last_opened_repo: Option<String>,
    #[uniffi(default = None)]
    pub last_clone_dir: Option<String>,
    #[uniffi(default = None)]
    pub repo_sort_mode: Option<String>,
    #[uniffi(default = None)]
    pub clone_sort_mode: Option<String>,
}

/// Mirrors [`leogit_core::git::RepoSync`] — the picker-badge summary:
/// pending pushes (`ahead`), pending pulls (`behind`), and uncommitted
/// changes (`dirty`), computed from `git status` headers without building a
/// file list. `fetched` reports whether a *requested* fetch reached the
/// remote, feeding the connectivity breaker.
#[uniffi::remote(Record)]
pub struct RepoSync {
    pub ahead: i32,
    pub behind: i32,
    pub has_remote: bool,
    pub fetched: bool,
    pub dirty: bool,
}

/// Mirrors [`leogit_core::git::RepoIdentifier`] — the `owner`/`name` pair
/// parsed out of a repository's remote URL, which is what a picker row is
/// labelled and searched by when the repository has one.
#[uniffi::remote(Record)]
pub struct RepoIdentifier {
    pub owner: String,
    pub name: String,
}

/// The shared configuration file, read fresh — the native client re-reads it
/// on every repo switch, like the Tauri client, so edits (from either
/// client) take effect without a restart.
///
/// # Errors
///
/// Returns [`GitError`] when the file exists but cannot be read or parsed.
/// A missing file is not an error — defaults are written and returned.
#[uniffi::export]
pub fn load_config() -> Result<Config, GitError> {
    config::load_config().map_err(GitError::from)
}

/// The range every numeric setting is clamped to — what a control's minimum,
/// maximum and step should be built from, so a form can't offer a value the
/// writer will silently correct.
#[must_use]
#[uniffi::export]
pub fn config_bounds() -> ConfigBounds {
    config::config_bounds()
}

/// Apply a field-wise patch to the configuration and return the result.
///
/// The only writer, and the reason a settings surface no longer has to
/// hand-roll load-fresh-then-edit: core reads, edits, normalizes and writes
/// under one lock, so a patch cannot revert a field it doesn't name. The
/// returned config is the normalized one — hand it straight back to the form
/// and an out-of-range entry corrects itself in place.
///
/// # Errors
///
/// Returns [`GitError`] when the file cannot be read, serialized or written.
#[uniffi::export]
pub fn patch_config(patch: ConfigPatch) -> Result<Config, GitError> {
    config::patch_config(patch).map_err(GitError::from)
}

/// The shared repos-state file. Corrupt state self-heals to defaults inside
/// core rather than erroring.
///
/// # Errors
///
/// Returns [`GitError`] when the state file cannot be read.
#[uniffi::export]
pub fn load_state() -> Result<ReposState, GitError> {
    config::load_state().map_err(GitError::from)
}

/// Apply a field-wise patch to the repos-state file and return the result.
/// The native client patches `last_opened_repo` on every switch, so the next
/// launch — of either client — restores it.
///
/// # Errors
///
/// Returns [`GitError`] when the state file cannot be read or written.
#[uniffi::export]
pub fn patch_state(patch: ReposStatePatch) -> Result<ReposState, GitError> {
    config::patch_state(patch).map_err(GitError::from)
}

/// Move `path` to the front of the most-recently-used list (de-duped,
/// capped inside core) and return the updated state.
///
/// # Errors
///
/// Returns [`GitError`] when the state file cannot be read or written.
#[uniffi::export]
pub fn record_recent_repo(path: String) -> Result<ReposState, GitError> {
    config::record_recent_repo(path).map_err(GitError::from)
}

/// Every repo the picker should list: a filesystem walk of the scan folders
/// (no git subprocesses) unioned with the recently-opened list, minus any
/// entry that no longer exists. An empty `scan_paths` falls back to core's
/// defaults; results are canonicalized and sorted.
///
/// Async over `spawn_blocking` for the same reason as [`repo_sync_status`]:
/// core's walk is synchronous and, over several roots at the configured
/// depth, stats enough directories to hold its thread for a noticeable
/// stretch. Swift's cooperative pool has one thread per core, so that wait
/// belongs on a blocking thread — the hop `#[tauri::command(async)]` performs
/// implicitly for the Tauri host.
///
/// # Errors
///
/// Returns [`GitError`] only on walk-level failures; unreadable folders are
/// skipped, not fatal, and an unreadable state file costs the MRU half rather
/// than the whole answer.
#[uniffi::export(async_runtime = "tokio")]
pub async fn known_repos(scan_paths: Vec<String>, max_depth: u32) -> Result<Vec<String>, GitError> {
    tokio::task::spawn_blocking(move || repos::known_repos(scan_paths, max_depth))
        .await
        .map_err(|join_error| GitError::Failed {
            message: format!("known_repos did not complete: {join_error}"),
        })?
        .map_err(GitError::from)
}

/// Narrow and rank the picker's rows against a typed query, strongest match
/// first. Ties keep the caller's own ordering, so an MRU or active-first
/// arrangement survives filtering rather than being scrambled by it.
///
/// A batch call rather than one per row: the rule is shared, and crossing the
/// bridge once per keystroke is what makes sharing it affordable.
#[must_use]
#[uniffi::export]
pub fn filter_repos(query: String, rows: Vec<RepoRow>, scan_folders: Vec<String>) -> Vec<String> {
    repos::filter_repos(&query, &rows, &scan_folders)
}

/// What cloning `raw_url` under `parent` would produce — the URL to hand git
/// (shorthand expanded), the folder name, and the path it lands at.
///
/// `None` means there is nothing cloneable in the input, which is also the
/// Clone button's enable condition: the preview and the button can't disagree
/// about whether the app is about to succeed.
#[must_use]
#[uniffi::export]
pub fn derive_clone_target(raw_url: String, parent: String) -> Option<CloneTarget> {
    repos::derive_clone_target(&raw_url, &parent)
}

/// Age the commit composer's opt-outs against the file list a status read just
/// produced, dropping the ones whose path has been gone longer than the grace
/// window. See `exclusions::reconcile_exclusions` for why the window exists.
///
/// `elapsed_ms` is wall-clock time since the previous call, not a tick count:
/// the poll's cadence changes with what the window is doing, so counting ticks
/// would make the window mean anything between 30 seconds and seven minutes.
#[must_use]
#[uniffi::export]
pub fn reconcile_exclusions(
    excluded: Vec<Exclusion>,
    present: Vec<String>,
    elapsed_ms: u32,
) -> Vec<Exclusion> {
    exclusions::reconcile_exclusions(&excluded, &present, elapsed_ms)
}

/// Where a clone of `repo_name` lands under `parent` — the GitHub tab's half
/// of the same rule, where the name comes from the selected repo rather than
/// from a URL.
#[must_use]
#[uniffi::export]
pub fn clone_target_path(parent: String, repo_name: String) -> Option<String> {
    repos::clone_target_path(&parent, &repo_name)
}

/// The folders discovery would actually walk for this configuration,
/// tilde-expanded — backs the picker's "no repositories found — we looked
/// here" empty state.
#[uniffi::export]
#[must_use]
pub fn effective_scan_paths(scan_paths: Vec<String>) -> Vec<String> {
    git::effective_scan_paths(scan_paths)
}

/// The `owner`/`name` a repository's remote URL parses to, or `None` when it
/// has no remote or the URL names no such pair — which is the picker's cue to
/// keep labelling that row with its folder name.
///
/// Async over `spawn_blocking` because it is one or two `git config` reads:
/// individually quick, but a picker asks for a row per repository and a
/// cooperative thread is too scarce to spend on any of them.
#[uniffi::export(async_runtime = "tokio")]
pub async fn repo_identifier(repo_path: String) -> Option<RepoIdentifier> {
    tokio::task::spawn_blocking(move || git::get_repo_identifier(repo_path))
        .await
        .unwrap_or(None)
}

/// Per-repo badge summary, optionally refreshing the remote-tracking refs
/// first. With `do_fetch`, the fetch runs under core's short background
/// timeouts (12 s cap) and a failure is swallowed — stale counts still come
/// back, with `fetched == false` for the breaker.
///
/// Async over `spawn_blocking` even though core's function is synchronous:
/// that fetch can hold its thread for the full timeout, which must never be
/// a Swift cooperative thread. The Tauri host makes the same hop implicitly
/// via `#[tauri::command(async)]`.
///
/// # Errors
///
/// Returns [`GitError`] when `git status` itself fails — a missing repo, a
/// deleted directory. Fetch failures are data (`fetched`), not errors.
#[uniffi::export(async_runtime = "tokio")]
pub async fn repo_sync_status(repo_path: String, do_fetch: bool) -> Result<RepoSync, GitError> {
    tokio::task::spawn_blocking(move || git::repo_sync_status(repo_path, do_fetch))
        .await
        .map_err(|join_error| GitError::Failed {
            message: format!("repo_sync_status did not complete: {join_error}"),
        })?
        .map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — launch target and update check
// ---------------------------------------------------------------------------

/// Resolve an argv list to the folder a `leogit <dir>` invocation points at.
///
/// Skips `args[0]` and takes the first non-flag argument, resolving a relative
/// path against `cwd`. `None` for a bare launch, a path that doesn't exist, or
/// one that isn't a directory — all of which just open the app. An existing
/// directory always resolves: `is_repo` is what distinguishes "open this
/// repository" from "offer to create one here".
///
/// The native host also feeds this the single path `AppKit` hands it in
/// `application(_:open:)`, as a one-element argv, so the folder a
/// double-click delivers is resolved by the same rule as one typed at a
/// prompt — including resolving a subdirectory up to its repository root.
///
/// Core's pending-target slot deliberately has no native export: it exists so
/// the Tauri host can stash a target before a window exists, and the native
/// client has a second source (`application(_:open:)`, which can fire at any
/// time) that a process global couldn't publish to the UI. One observable
/// Swift store owns both instead.
#[must_use]
#[uniffi::export]
pub fn resolve_launch_target(args: Vec<String>, cwd: String) -> Option<LaunchTarget> {
    launch::resolve_launch_target(&args, std::path::Path::new(&cwd))
}

/// Ask GitHub Releases whether a version newer than this build exists.
/// `None` means this build is current — or that the newer release has no
/// artifact for this platform yet, which core withholds rather than offering
/// an update the installer could not complete.
///
/// # Errors
///
/// Returns [`GitError`] when the check itself fails (offline, rate-limited,
/// GitHub down). That is "couldn't check", not "no update": the caller retries
/// quietly and shows the user nothing.
#[uniffi::export(async_runtime = "tokio")]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, GitError> {
    update::check_for_update().await.map_err(GitError::from)
}

// ---------------------------------------------------------------------------
// Exported functions — embedded terminal
// ---------------------------------------------------------------------------
//
// The PTY lives entirely in core (portable-pty): sessions are keyed by a
// synthetic id, and each one owns a reader thread that streams decoded UTF-8
// through the `EventSink` seam. `TerminalSink` adapts that seam to a
// Swift-implemented `TerminalEventListener` — its own foreign trait rather
// than variants bolted onto `SyncProgressListener`, because the two event
// shapes share nothing and each sink is scoped to what its operation can
// emit. Everything here is synchronous: core spawns plain OS threads, so no
// tokio context is involved (unlike fetch/pull/push).
//
// Deliberately NOT exported (no native consumers — the dead-surface rule):
// `terminal_pty_info`, whose `backend`/`build_number` describe Windows
// ConPTY quirks xterm.js must know about before construction and which is
// all-`None` on macOS.

/// Swift-implemented receiver for one terminal session's events.
///
/// `on_output` arrives on the session's PTY reader thread — never the main
/// one — once per read (4 KiB max) with no throttling, so implementations
/// must return quickly (buffer and hop) rather than render in place. `data`
/// is decoded UTF-8 text, not raw bytes: core's streaming decoder holds
/// split multi-byte sequences across chunk boundaries, so feeding
/// `data.utf8` to an emulator is lossless.
///
/// `on_closed` is emitted by the same reader thread when the child exits —
/// on its own or via [`close_terminal`] — after core has already dropped the
/// session and reaped the child. It is the only end-of-session signal, and
/// `exit` carries the child's status: a clean exit closes the panel, anything
/// else keeps the dead terminal on screen with the reason (a kill surfaces as
/// a fatal signal, `"Hangup"`, since the escalation starts with SIGHUP).
#[uniffi::export(foreign)]
pub trait TerminalEventListener: Send + Sync {
    /// Deliver one chunk of decoded PTY output.
    fn on_output(&self, pid: u32, data: String);
    /// The session's child exited with `exit`; `pid` is no longer valid.
    fn on_closed(&self, pid: u32, exit: TerminalExit);
}

/// Adapts core's [`EventSink`] to a [`TerminalEventListener`]. Git progress
/// is ignored: this sink is only ever handed to `start_terminal`, whose
/// session emits nothing else.
struct TerminalSink {
    listener: Arc<dyn TerminalEventListener>,
}

impl EventSink for TerminalSink {
    fn emit(&self, event: CoreEvent) {
        match event {
            CoreEvent::TerminalOutput { pid, data } => self.listener.on_output(pid, data),
            CoreEvent::TerminalClosed { pid, exit } => self.listener.on_closed(pid, exit),
            CoreEvent::GitProgress(_) => {}
        }
    }
}

/// Mirrors [`leogit_core::events::TerminalExit`] — how a session's child
/// ended, reaped by core's reader thread after PTY EOF. `exit_code` is a
/// fabricated `1` when `signal` names the fatal signal instead.
#[uniffi::remote(Record)]
pub struct TerminalExit {
    pub exit_code: u32,
    pub signal: Option<String>,
}

#[uniffi::remote(Record)]
pub struct StartedTerminal {
    /// Synthetic session id — deliberately not the OS pid, so the UI never
    /// holds a reusable process handle. The key for write/resize/close and
    /// the id the listener's events carry.
    pub pid: u32,
    /// Id of the shell that actually launched (`"default"`, `"zsh"`, …).
    pub shell_id: String,
    /// Human-readable shell name for the panel header, so what launched is
    /// never a guess.
    pub shell_label: String,
}

/// Spawn a shell in a fresh PTY at `repo_path`, 80×24 until the first
/// resize. `shell_id` is the shared config's `terminal_shell`; `None` — or
/// an id whose shell is not installed here — resolves to the best shell on
/// this machine (`$SHELL`, then zsh/bash/fish/sh). The session's reader
/// thread holds `listener` until the child exits.
///
/// # Errors
///
/// Returns [`GitError`] when the PTY cannot be opened or the shell fails to
/// spawn.
#[uniffi::export]
pub fn start_terminal(
    listener: Arc<dyn TerminalEventListener>,
    repo_path: String,
    shell_id: Option<String>,
) -> Result<StartedTerminal, GitError> {
    terminal::start_terminal(Arc::new(TerminalSink { listener }), &repo_path, shell_id)
        .map_err(GitError::from)
}

/// Write keystrokes (or a paste) to the session's PTY, flushed immediately.
///
/// # Errors
///
/// Returns [`GitError`] when the session is gone or the PTY write fails.
#[uniffi::export]
pub fn write_terminal(pid: u32, data: String) -> Result<(), GitError> {
    terminal::write_terminal(pid, &data).map_err(GitError::from)
}

/// Propagate the emulator's grid size to the PTY, which delivers `SIGWINCH`
/// to the child.
///
/// # Errors
///
/// Returns [`GitError`] when the session is gone or the resize fails.
#[uniffi::export]
pub fn resize_terminal(pid: u32, cols: u16, rows: u16) -> Result<(), GitError> {
    terminal::resize_terminal(pid, cols, rows).map_err(GitError::from)
}

/// Kill the session's child. The reader thread then reaches EOF and emits
/// `on_closed`, exactly as when the shell exits on its own — callers treat
/// that event, never this return, as the end of the session. A killed shell
/// reports its fatal signal in the close payload rather than a clean exit.
///
/// # Errors
///
/// Returns [`GitError`] only on a poisoned session registry.
#[uniffi::export]
pub fn close_terminal(pid: u32) -> Result<(), GitError> {
    terminal::close_terminal(pid).map_err(GitError::from)
}

/// Mirrors [`leogit_core::shell::ShellOption`] — one row of the Settings
/// window's shell picker. `id` is the stable value persisted in
/// `config.terminal_shell` (`"default"` is the `$SHELL` row, not a
/// sentinel — the *absent* preference is `terminal_shell: None`, which the
/// picker renders as "Automatic").
#[uniffi::remote(Record)]
pub struct ShellOption {
    pub id: String,
    pub label: String,
    pub path: String,
    pub args: Vec<String>,
}

/// The shells installed on this machine, best first — probe-based, so every
/// row is launchable, and never empty (`sh` at worst). Feeds the native
/// Settings shell picker the same way `list_shells` feeds the Tauri one; an
/// id that stops resolving (uninstalled shell) is normalized by the picker
/// to "Automatic", and `start_terminal` would fall back to the best shell
/// anyway.
#[uniffi::export]
#[must_use]
pub fn list_shells() -> Vec<ShellOption> {
    shell::list_shells()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
            let payload =
                get_parsed_diff(repo.clone(), file.clone(), false, lean_diff()).expect("diff");
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

    /// Runs `git` in `dir` and returns trimmed stdout, panicking on failure.
    fn run_git_stdout(dir: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// The commit flow against a real throwaway repository, exactly as the
    /// Swift client drives it: status → check a subset of files →
    /// `format_commit_message` → `commit`. Pins that only the selected file
    /// lands in the commit and that the unselected one survives as a change.
    #[test]
    fn commit_flow_commits_only_the_selected_files() {
        let dir = std::env::temp_dir().join(format!("leogit-ffi-commit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        run_git(&dir, &["init"]);
        run_git(&dir, &["config", "user.name", "t"]);
        run_git(&dir, &["config", "user.email", "t@t"]);
        std::fs::write(dir.join("a.txt"), "one\n").expect("write");
        std::fs::write(dir.join("b.txt"), "two\n").expect("write");

        let repo = dir.to_string_lossy().to_string();
        let status = get_status(repo.clone()).expect("status");
        assert_eq!(status.files.len(), 2, "two untracked files");

        let selected: Vec<FileEntry> = status
            .files
            .into_iter()
            .filter(|f| f.path == "a.txt")
            .collect();
        let message = format_commit_message(
            "Add a.txt".to_string(),
            "Only half the working tree.".to_string(),
            vec!["T <t@t>".to_string()],
        );
        assert_eq!(
            message,
            "Add a.txt\n\nOnly half the working tree.\n\nCo-authored-by: T <t@t>"
        );
        commit(repo.clone(), message, selected, None).expect("commit");

        let subject = run_git_stdout(&dir, &["log", "-1", "--format=%s"]);
        assert_eq!(subject, "Add a.txt");
        let committed = run_git_stdout(&dir, &["ls-tree", "-r", "--name-only", "HEAD"]);
        assert_eq!(committed, "a.txt", "b.txt stays out of the commit");

        let after = get_status(repo.clone()).expect("status after commit");
        assert_eq!(after.files.len(), 1, "b.txt is still a pending change");
        assert_eq!(after.files[0].path, "b.txt");

        let err = commit(repo, "empty".to_string(), Vec::new(), None).unwrap_err();
        assert!(
            matches!(err, GitError::Failed { ref message } if message.contains("no files")),
            "empty selection surfaces core's own error"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Changes-tab row menu's two write actions against a real repository:
    /// discard restores a tracked file to `HEAD`, and the ignore pair appends
    /// escaped literal paths and raw globs to `.gitignore` without duplicating
    /// rules that are already there.
    ///
    /// Discarding an *untracked* file is deliberately left out: core moves it
    /// to the OS trash, and a test has no business putting anything in the
    /// user's Trash. Core owns that branch and covers it itself.
    #[test]
    fn discard_restores_tracked_files_and_ignore_appends_rules() {
        let (dir, repo, _) = seeded_repo("context-discard");

        std::fs::write(dir.join("base.txt"), "edited\n").expect("write");
        let status = get_status(repo.clone()).expect("status");
        assert_eq!(status.files.len(), 1, "the edit is the only change");

        discard_files(repo.clone(), status.files).expect("discard");
        assert_eq!(
            std::fs::read_to_string(dir.join("base.txt")).expect("read"),
            "base\n",
            "the committed content is back"
        );
        assert!(
            get_status(repo.clone()).expect("status").files.is_empty(),
            "the working tree is clean again"
        );

        // Glob metacharacters in a literal path are escaped, so the rule
        // matches that one file rather than acting as a pattern.
        ignore_paths(repo.clone(), vec!["logs/weird[1].txt".to_string()]).expect("ignore path");
        // Raw patterns go through untouched, and a repeat is a no-op.
        append_to_gitignore(repo.clone(), vec!["*.log".to_string()]).expect("ignore extension");
        append_to_gitignore(repo.clone(), vec!["*.log".to_string()]).expect("ignore again");

        let rules = std::fs::read_to_string(dir.join(".gitignore")).expect("read .gitignore");
        let lines: Vec<&str> = rules.lines().collect();
        assert_eq!(
            lines,
            vec![r"logs/weird\[1\].txt", "*.log"],
            "escaped path first, glob verbatim, no duplicate"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The History row menu's two navigational actions: checking out an older
    /// commit detaches HEAD at that sha, and undoing the last commit drops it
    /// from history while its changes survive in the working tree. Also pins
    /// core's refusal to undo the initial commit, which is what makes the
    /// menu item safe to offer on a one-commit repository.
    #[test]
    fn checkout_commit_detaches_and_undo_keeps_the_changes() {
        let (dir, repo, branch) = seeded_repo("context-history");

        std::fs::write(dir.join("second.txt"), "second\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "add second"]);

        let log = get_log(
            repo.clone(),
            LogOptions {
                max_count: 10,
                skip: 0,
            },
        )
        .expect("log");
        assert_eq!(log.len(), 2, "two commits");
        let root = log[1].sha.clone();

        checkout_commit(repo.clone(), root.clone()).expect("checkout commit");
        let detached = get_status(repo.clone()).expect("status detached");
        assert!(detached.detached, "HEAD is detached");
        assert_eq!(detached.head_sha, root, "detached at the requested commit");
        assert!(
            !dir.join("second.txt").exists(),
            "the older commit's tree is what's checked out"
        );

        switch_branch(repo.clone(), branch).expect("reattach");
        undo_last_commit(repo.clone()).expect("undo");

        let after = get_log(
            repo.clone(),
            LogOptions {
                max_count: 10,
                skip: 0,
            },
        )
        .expect("log after undo");
        assert_eq!(after.len(), 1, "the commit is gone from history");
        let status = get_status(repo.clone()).expect("status after undo");
        assert_eq!(
            status
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["second.txt"],
            "its changes came back as a pending change"
        );

        let err = undo_last_commit(repo).unwrap_err();
        assert!(
            matches!(err, GitError::Failed { ref message } if message.contains("initial commit")),
            "core refuses to undo the root commit: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Seed a throwaway repository with one commit and return its path plus
    /// the default branch name (main or master, per the host's git config).
    fn seeded_repo(tag: &str) -> (std::path::PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("leogit-ffi-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        run_git(&dir, &["init"]);
        run_git(&dir, &["config", "user.name", "t"]);
        run_git(&dir, &["config", "user.email", "t@t"]);
        std::fs::write(dir.join("base.txt"), "base\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "init"]);
        let repo = dir.to_string_lossy().to_string();
        let default = run_git_stdout(&dir, &["symbolic-ref", "--short", "HEAD"]);
        (dir, repo, default)
    }

    /// The History detail reads exactly as the Swift client drives them:
    /// select a commit → `get_commit_files` + `get_commit_stats`, then per
    /// file `get_commit_diff` → `parse_diff` → `tokenize_diff` with
    /// `BlobSource::Commit`. Pins rename detection crossing the bridge
    /// (`orig_path`), binary files counting zero in the stats, and the
    /// commit-sourced tokenizer path — none of which had coverage before.
    #[test]
    fn commit_detail_reads_files_stats_and_per_file_diffs() {
        let (dir, repo, _) = seeded_repo("commit-detail");
        run_git(&dir, &["mv", "base.txt", "renamed.txt"]);
        std::fs::create_dir_all(dir.join("src")).expect("mkdir");
        std::fs::write(
            dir.join("src/new.rs"),
            "pub fn added() {}\npub fn more() {}\n",
        )
        .expect("write");
        std::fs::write(dir.join("bin.dat"), [0u8, 159, 146, 150]).expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "second"]);
        let sha = run_git_stdout(&dir, &["rev-parse", "HEAD"]);

        let detail = get_commit_detail(repo.clone(), sha.clone()).expect("commit detail");
        let files = detail.files;
        assert_eq!(files.len(), 3, "rename + source file + binary: {files:?}");
        let renamed = files
            .iter()
            .find(|f| f.path == "renamed.txt")
            .expect("renamed entry");
        assert!(matches!(renamed.status, FileStatus::Renamed));
        assert_eq!(renamed.orig_path.as_deref(), Some("base.txt"));
        let added = files
            .iter()
            .find(|f| f.path == "src/new.rs")
            .expect("added entry");
        assert!(matches!(added.status, FileStatus::New));

        // The pure rename moves no lines and numstat reports the binary file
        // as `-`, so the two source lines are the whole count.
        assert_eq!((detail.stats.additions, detail.stats.deletions), (2, 0));

        let payload =
            get_parsed_commit_diff(repo.clone(), sha.clone(), added.path.clone(), lean_diff())
                .expect("diff");
        assert_eq!(payload.additions, 2);
        let flat: usize = payload.file_diff.hunks.iter().map(|h| h.lines.len()).sum();
        let tokens = tokenize_diff(
            payload.file_diff,
            Some(BlobSource::Commit {
                repo_path: repo.clone(),
                sha: sha.clone(),
            }),
        );
        assert_eq!(tokens.len(), flat, "one token line per diff line");
        assert!(
            tokens.iter().any(|line| !line.is_empty()),
            "rust source read at the commit produces tokens"
        );

        let whole = get_parsed_commit_diff(repo, sha, String::new(), lean_diff())
            .expect("whole-commit diff");
        let paths: Vec<&str> = whole
            .file_diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter_map(|l| l.text.as_deref())
            .collect();
        assert!(!paths.is_empty(), "the whole-commit diff parses into hunks");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The branch lifecycle exactly as the Swift client drives it: list →
    /// create + switch (the two-call "New branch" flow) → list shows the new
    /// current branch → switch back → delete → gone.
    #[test]
    fn branch_flow_creates_switches_and_deletes() {
        let (dir, repo, default) = seeded_repo("branch");

        let initial = list_branches(repo.clone()).expect("list");
        assert_eq!(initial.len(), 1);
        assert!(initial[0].is_current && !initial[0].is_remote);
        assert_eq!(initial[0].name, default);

        create_branch(repo.clone(), "feature".to_string(), String::new()).expect("create");
        switch_branch(repo.clone(), "feature".to_string()).expect("switch");
        let after_create = list_branches(repo.clone()).expect("list");
        assert_eq!(after_create.len(), 2);
        let current: Vec<_> = after_create.iter().filter(|b| b.is_current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].name, "feature");

        // Deleting the checked-out branch must refuse, like git itself.
        let err = delete_branch(repo.clone(), "feature".to_string()).unwrap_err();
        assert!(matches!(err, GitError::Failed { .. }));

        switch_branch(repo.clone(), default.clone()).expect("switch back");
        delete_branch(repo.clone(), "feature".to_string()).expect("delete");
        let after_delete = list_branches(repo).expect("list");
        assert_eq!(after_delete.len(), 1);
        assert_eq!(after_delete[0].name, default);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The three merge outcomes end-to-end: a fast-forward `merge_branch`, the
    /// two-call squash flow, and a conflict — which must come back as data
    /// (`success == false` + conflicted paths), flip the status's `merging`
    /// flag, and clean up via `merge_abort`.
    #[test]
    fn merge_flow_fast_forwards_squashes_and_surfaces_conflicts() {
        let (dir, repo, default) = seeded_repo("merge");

        // Fast-forward: default gains no commits while `feature` adds one.
        create_branch(repo.clone(), "feature".to_string(), String::new()).expect("create");
        switch_branch(repo.clone(), "feature".to_string()).expect("switch");
        std::fs::write(dir.join("feature.txt"), "f\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "feat"]);
        switch_branch(repo.clone(), default.clone()).expect("switch back");

        let preview = count_commits_to_merge(repo.clone(), "feature".to_string()).expect("count");
        assert_eq!(preview, 1, "feature brings exactly one commit");
        let ff = merge_branch(repo.clone(), "feature".to_string()).expect("merge");
        assert!(ff.success && ff.fast_forward && ff.conflicts.is_empty());

        // Squash: stage the branch's changes, then commit with git's message.
        create_branch(repo.clone(), "feature2".to_string(), String::new()).expect("create");
        switch_branch(repo.clone(), "feature2".to_string()).expect("switch");
        std::fs::write(dir.join("second.txt"), "s\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "second"]);
        switch_branch(repo.clone(), default.clone()).expect("switch back");
        let squash = merge_squash(repo.clone(), "feature2".to_string()).expect("squash");
        assert!(squash.success && !squash.fast_forward);
        commit_squash_merge(repo.clone()).expect("squash commit");
        let subject = run_git_stdout(&dir, &["log", "-1", "--format=%s"]);
        assert_eq!(subject, "Squashed commit of the following:");

        // Conflict: both sides edit base.txt.
        create_branch(repo.clone(), "clash".to_string(), String::new()).expect("create");
        switch_branch(repo.clone(), "clash".to_string()).expect("switch");
        std::fs::write(dir.join("base.txt"), "theirs\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "theirs"]);
        switch_branch(repo.clone(), default).expect("switch back");
        std::fs::write(dir.join("base.txt"), "ours\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "ours"]);

        let conflict = merge_branch(repo.clone(), "clash".to_string()).expect("merge runs");
        assert!(!conflict.success, "conflict is data, not an Err");
        assert_eq!(conflict.conflicts, ["base.txt"]);
        assert!(conflict.error_message.is_some());
        assert!(
            get_status(repo.clone()).expect("status").merging,
            "MERGE_HEAD exists, and the status the UI reads says so"
        );

        merge_abort(repo.clone()).expect("abort");
        assert!(
            !get_status(repo).expect("status").merging,
            "abort clears the merge"
        );
        let restored = std::fs::read_to_string(dir.join("base.txt")).expect("read");
        assert_eq!(restored, "ours\n", "abort restores the pre-merge tree");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Rust stand-in for the Swift listener: collects every tick so tests
    /// can assert the callback seam actually fires.
    struct CollectingListener {
        events: std::sync::Mutex<Vec<SyncProgress>>,
    }

    impl CollectingListener {
        fn arc() -> Arc<Self> {
            Arc::new(Self {
                events: std::sync::Mutex::new(Vec::new()),
            })
        }
    }

    impl SyncProgressListener for CollectingListener {
        fn on_progress(&self, progress: SyncProgress) {
            self.events.lock().expect("lock").push(progress);
        }
    }

    /// Create a bare "origin" next to the temp repos and wire `dir` to it.
    fn bare_origin(tag: &str, dir: &Path) -> std::path::PathBuf {
        let bare = std::env::temp_dir().join(format!(
            "leogit-ffi-{tag}-origin-{}.git",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&bare);
        std::fs::create_dir_all(&bare).expect("bare dir");
        run_git(&bare, &["init", "--bare"]);
        run_git(
            dir,
            &["remote", "add", "origin", bare.to_str().expect("utf-8")],
        );
        bare
    }

    /// Clone `bare` to a sibling working copy with commit identity configured.
    fn cloned_workmate(tag: &str, bare: &Path) -> std::path::PathBuf {
        let clone = std::env::temp_dir().join(format!("leogit-ffi-{tag}-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&clone);
        run_git(
            &std::env::temp_dir(),
            &[
                "clone",
                bare.to_str().expect("utf-8"),
                clone.to_str().expect("utf-8"),
            ],
        );
        run_git(&clone, &["config", "user.name", "t"]);
        run_git(&clone, &["config", "user.email", "t@t"]);
        clone
    }

    /// The sync flow exactly as the Swift client drives it: publish the branch
    /// (`push --set-upstream`) to a fresh bare origin — with progress ticks
    /// crossing the callback seam — then `fetch` + `pull` a commit made by a
    /// second clone.
    #[tokio::test]
    async fn sync_flow_publishes_fetches_and_pulls() {
        let (dir, repo, default) = seeded_repo("sync");
        assert_eq!(
            get_remote(repo.clone()).expect("remote"),
            None,
            "a repo with no remote says so rather than inventing one"
        );
        let bare = bare_origin("sync", &dir);

        let before = get_status(repo.clone()).expect("status");
        assert!(
            before.has_remote && !before.has_upstream,
            "publish-branch state: a remote exists but the branch is untracked"
        );

        let listener = CollectingListener::arc();
        push(
            repo.clone(),
            "origin".to_string(),
            default.clone(),
            true,
            false,
            listener.clone(),
        )
        .await
        .expect("first push");
        assert!(
            !listener.events.lock().expect("lock").is_empty(),
            "progress ticks crossed the callback seam"
        );

        let published = get_status(repo.clone()).expect("status");
        assert!(published.has_upstream, "--set-upstream took");
        assert_eq!((published.ahead, published.behind), (0, 0));

        // A second clone advances the remote…
        let clone_b = cloned_workmate("sync", &bare);
        std::fs::write(clone_b.join("from-b.txt"), "b\n").expect("write");
        run_git(&clone_b, &["add", "."]);
        run_git(&clone_b, &["commit", "-m", "from b"]);
        run_git(&clone_b, &["push"]);

        // …fetch sees it as behind without touching the working tree…
        fetch(repo.clone(), "origin".to_string(), false)
            .await
            .expect("fetch");
        let behind = get_status(repo.clone()).expect("status");
        assert_eq!((behind.ahead, behind.behind), (0, 1));
        assert!(
            !dir.join("from-b.txt").exists(),
            "fetch leaves the working tree alone"
        );

        // …and pull fast-forwards onto it.
        pull(
            repo.clone(),
            "origin".to_string(),
            CollectingListener::arc(),
        )
        .await
        .expect("pull");
        let after = get_status(repo).expect("status");
        assert_eq!((after.ahead, after.behind), (0, 0));
        assert!(
            dir.join("from-b.txt").exists(),
            "pull updated the working tree"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
        let _ = std::fs::remove_dir_all(&clone_b);
    }

    /// Force-push semantics end to end: a diverged branch is rejected by a
    /// plain push, rejected again by `--force-with-lease` while the
    /// remote-tracking ref is stale, and accepted after a fetch — the exact
    /// flow behind the UI's "Force Push (with Lease)…" confirmation.
    #[tokio::test]
    async fn force_with_lease_rejects_stale_then_succeeds_after_fetch() {
        let (dir, repo, default) = seeded_repo("lease");
        let bare = bare_origin("lease", &dir);
        run_git(&dir, &["push", "--set-upstream", "origin", &default]);
        let clone_b = cloned_workmate("lease", &bare);

        // Diverge: B pushes "theirs" while A commits "ours" locally.
        std::fs::write(clone_b.join("theirs.txt"), "t\n").expect("write");
        run_git(&clone_b, &["add", "."]);
        run_git(&clone_b, &["commit", "-m", "theirs"]);
        run_git(&clone_b, &["push"]);
        std::fs::write(dir.join("ours.txt"), "o\n").expect("write");
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "ours"]);

        // Plain push: non-fast-forward, rejected.
        let rejected = push(
            repo.clone(),
            "origin".to_string(),
            default.clone(),
            false,
            false,
            CollectingListener::arc(),
        )
        .await
        .unwrap_err();
        assert!(matches!(rejected, GitError::Failed { .. }));

        // Lease with a stale remote-tracking ref: still rejected — the lease
        // means "the remote is where I last saw it", and we haven't seen B's
        // push yet.
        let stale = push(
            repo.clone(),
            "origin".to_string(),
            default.clone(),
            false,
            true,
            CollectingListener::arc(),
        )
        .await
        .unwrap_err();
        assert!(matches!(stale, GitError::Failed { .. }));

        // After a fetch the lease matches, and the forced push wins.
        fetch(repo.clone(), "origin".to_string(), false)
            .await
            .expect("fetch");
        push(
            repo.clone(),
            "origin".to_string(),
            default.clone(),
            false,
            true,
            CollectingListener::arc(),
        )
        .await
        .expect("force with lease");

        let local_head = run_git_stdout(&dir, &["rev-parse", "HEAD"]);
        let remote_head = run_git_stdout(&bare, &["rev-parse", &default]);
        assert_eq!(remote_head, local_head, "the remote now points at ours");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
        let _ = std::fs::remove_dir_all(&clone_b);
    }

    /// The mapping now lives in core, and the bridge is the delegation it
    /// always claimed to be. What this pins is that the shared record still
    /// carries what each provider needs across the boundary — the only part
    /// this crate owns.
    #[test]
    fn ai_config_crosses_with_the_knobs_each_provider_reads() {
        let mut cfg = config::Config::default();
        cfg.claude.model = Some("sonnet".to_string());
        cfg.ollama.model = Some("llama3".to_string());

        let claude: AiProviderConfig = ai::provider_config(&cfg);
        assert_eq!(claude.provider, "claude");
        assert_eq!(claude.model.as_deref(), Some("sonnet"));
        assert_eq!(claude.timeout_secs, config::AI_TIMEOUT_SECS.fallback);

        cfg.ai_provider = "ollama".to_string();
        let ollama: AiProviderConfig = ai::provider_config(&cfg);
        assert_eq!(ollama.provider, "ollama");
        assert_eq!(ollama.model.as_deref(), Some("llama3"));
        assert_eq!(
            ollama.base_url.as_deref(),
            Some("http://localhost:11434"),
            "the ollama URL travels as base_url"
        );
    }

    /// `get_selected_diff` exactly as the composer's Generate drives it: the
    /// checked files' combined diff — and only theirs — with an empty
    /// selection yielding the empty string core's generate then rejects.
    #[test]
    fn selected_diff_combines_only_the_given_files() {
        let (dir, repo, _default) = seeded_repo("seldiff");
        std::fs::write(dir.join("base.txt"), "changed\n").expect("write");
        std::fs::write(dir.join("fresh.txt"), "new\n").expect("write");

        let status = get_status(repo.clone()).expect("status");
        assert_eq!(status.files.len(), 2, "one modified + one untracked");

        let all = get_selected_diff(repo.clone(), status.files.clone()).expect("diff");
        assert!(
            all.contains("base.txt") && all.contains("fresh.txt"),
            "both checked files contribute hunks"
        );

        let only_fresh: Vec<FileEntry> = status
            .files
            .into_iter()
            .filter(|f| f.path == "fresh.txt")
            .collect();
        let single = get_selected_diff(repo.clone(), only_fresh).expect("diff");
        assert!(
            single.contains("fresh.txt") && !single.contains("base.txt"),
            "unchecked files stay out of the AI's input"
        );

        assert_eq!(get_selected_diff(repo, Vec::new()).expect("diff"), "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn bare_ai_config(provider: &str) -> AiProviderConfig {
        AiProviderConfig {
            provider: provider.to_string(),
            model: None,
            base_url: None,
            timeout_secs: 120,
        }
    }

    /// What the native client asks for: the line model, nothing rendered.
    fn lean_diff() -> DiffOptions {
        DiffOptions {
            html: false,
            side_by_side: false,
            show_anyway: false,
        }
    }

    /// The generate export's own guards — the paths that need no AI backend
    /// installed. Also proves the async export is awaitable outside the FFI
    /// (the `async_runtime` wrapper only applies to FFI-driven calls).
    #[tokio::test]
    async fn generate_commit_message_rejects_bad_input_without_a_backend() {
        let empty = generate_commit_message(
            "   ".to_string(),
            "claude".to_string(),
            bare_ai_config("claude"),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(empty, GitError::Failed { ref message } if message.contains("no files selected")),
            "a whitespace-only diff is refused before any provider runs"
        );

        let unknown = generate_commit_message(
            "diff --git a/x b/x".to_string(),
            "copilot".to_string(),
            bare_ai_config("copilot"),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(unknown, GitError::Failed { ref message } if message.contains("Unknown AI provider"))
        );
    }

    /// An unknown provider can't be persisted, because normalization folds it
    /// onto claude before the write — a rule that now protects every writer
    /// rather than the one export that happened to validate. Asserted against
    /// the pure normalizer so the test stays away from the real
    /// `config.toml`, like the other config paths here.
    #[test]
    fn an_unknown_provider_cannot_reach_the_config_file() {
        let normalized = config::Config {
            ai_provider: "copilot".to_string(),
            ..config::Config::default()
        }
        .normalized();
        assert_eq!(normalized.ai_provider, "claude");
    }

    /// The picker's row list exactly as it is driven: repos at the scan
    /// root's first level and nested deeper are both found, plain folders and
    /// missing scan roots contribute nothing, and `effective_scan_paths`
    /// reports tilde-expanded folders.
    ///
    /// Asserted by membership rather than by count: `known_repos` also unions
    /// in the shared MRU, which is the machine's real `repos-state.json` —
    /// exactly the file this crate's tests otherwise stay away from. Core's
    /// own `union_known_repos` tests pin the merge rule against fixtures; what
    /// this covers is the discovery half reaching the bridge intact.
    #[tokio::test]
    async fn discovery_finds_repos_and_reports_scan_folders() {
        let root = std::env::temp_dir().join(format!("leogit-ffi-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let direct = root.join("direct");
        let nested = root.join("group").join("nested");
        std::fs::create_dir_all(&direct).expect("dir");
        std::fs::create_dir_all(&nested).expect("dir");
        std::fs::create_dir_all(root.join("plain")).expect("dir");
        run_git(&direct, &["init"]);
        run_git(&nested, &["init"]);

        let missing = root.join("not-there");
        let repos = known_repos(
            vec![
                root.to_string_lossy().into_owned(),
                missing.to_string_lossy().into_owned(),
            ],
            3,
        )
        .await
        .expect("discover");
        // Compare by suffix: results are canonicalized, and /tmp resolves
        // through /private on macOS.
        assert!(repos.iter().any(|r| r.ends_with("/direct")));
        assert!(repos.iter().any(|r| r.ends_with("/nested")));
        assert!(
            !repos.iter().any(|r| r.ends_with("/plain")),
            "a folder that is not a repo contributes nothing"
        );
        assert!(
            !repos.iter().any(|r| r.ends_with("/not-there")),
            "a missing scan root contributes nothing"
        );

        let folders = effective_scan_paths(vec!["~/Dev".to_string()]);
        assert_eq!(folders.len(), 1);
        assert!(
            folders[0].ends_with("/Dev") && !folders[0].starts_with('~'),
            "scan folders come back tilde-expanded: {folders:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The picker-badge summary through every state the rows render: no
    /// remote (badges suppressed, fetch skipped), in sync, dirty, ahead —
    /// and behind, which only a `do_fetch` sweep can see, pinning that the
    /// fetch path actually refreshes the remote-tracking ref.
    #[tokio::test]
    async fn repo_sync_status_reports_dirty_ahead_and_behind() {
        let (dir, repo, default) = seeded_repo("badges");

        let no_remote = repo_sync_status(repo.clone(), true).await.expect("sync");
        assert!(!no_remote.has_remote);
        assert!(
            no_remote.fetched,
            "no remote to reach counts as nothing-to-report, not a failure"
        );
        assert_eq!((no_remote.ahead, no_remote.behind), (0, 0));
        assert!(!no_remote.dirty);

        let bare = bare_origin("badges", &dir);
        run_git(&dir, &["push", "--set-upstream", "origin", &default]);
        let in_sync = repo_sync_status(repo.clone(), false).await.expect("sync");
        assert!(in_sync.has_remote && !in_sync.dirty);
        assert_eq!((in_sync.ahead, in_sync.behind), (0, 0));

        std::fs::write(dir.join("wip.txt"), "wip\n").expect("write");
        let dirty = repo_sync_status(repo.clone(), false).await.expect("sync");
        assert!(dirty.dirty, "an untracked file lights the dirty dot");

        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "wip"]);
        let ahead = repo_sync_status(repo.clone(), false).await.expect("sync");
        assert!(!ahead.dirty, "committed, so the dot goes out");
        assert_eq!((ahead.ahead, ahead.behind), (1, 0));

        run_git(&dir, &["push"]);
        let clone_b = cloned_workmate("badges", &bare);
        std::fs::write(clone_b.join("from-b.txt"), "b\n").expect("write");
        run_git(&clone_b, &["add", "."]);
        run_git(&clone_b, &["commit", "-m", "from b"]);
        run_git(&clone_b, &["push"]);

        let stale = repo_sync_status(repo.clone(), false).await.expect("sync");
        assert_eq!(
            stale.behind, 0,
            "a fetch-less sweep reads only local refs, so B's push is invisible"
        );
        let fetched = repo_sync_status(repo, true).await.expect("sync");
        assert!(fetched.fetched, "the requested fetch reached the remote");
        assert_eq!((fetched.ahead, fetched.behind), (0, 1));

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
        let _ = std::fs::remove_dir_all(&clone_b);
    }

    /// Collects one session's terminal events across the callback seam, the
    /// way the Swift relay will.
    #[derive(Default)]
    struct TerminalProbe {
        output: std::sync::Mutex<String>,
        closed: std::sync::Mutex<Vec<(u32, TerminalExit)>>,
    }

    impl TerminalProbe {
        fn exit_for(&self, pid: u32) -> Option<TerminalExit> {
            self.closed
                .lock()
                .unwrap()
                .iter()
                .find(|(closed_pid, _)| *closed_pid == pid)
                .map(|(_, exit)| exit.clone())
        }
    }

    impl TerminalEventListener for TerminalProbe {
        fn on_output(&self, _pid: u32, data: String) {
            self.output.lock().unwrap().push_str(&data);
        }
        fn on_closed(&self, pid: u32, exit: TerminalExit) {
            self.closed.lock().unwrap().push((pid, exit));
        }
    }

    /// Polls `ready` until it holds or `secs` elapse — the PTY reader thread
    /// delivers asynchronously, so the tests wait rather than sleep blind.
    fn wait_until(secs: u64, mut ready: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while std::time::Instant::now() < deadline {
            if ready() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    /// A real PTY session end-to-end through the exports: spawn `sh` in a
    /// temp dir, type a command, watch its output arrive via `on_output`,
    /// exit, and watch `on_closed` retire the pid.
    #[test]
    fn terminal_session_streams_output_and_reports_exit() {
        let dir = std::env::temp_dir().join(format!("leogit-ffi-term-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let probe = Arc::new(TerminalProbe::default());
        let started = start_terminal(
            probe.clone(),
            dir.to_string_lossy().to_string(),
            Some("sh".to_string()),
        )
        .expect("terminal starts");
        assert!(started.pid >= 1, "synthetic ids start at 1");
        assert!(
            !started.shell_label.is_empty(),
            "the launch names its shell"
        );

        write_terminal(started.pid, "printf 'seam:%s\\n' ok; exit 7\n".to_string())
            .expect("write reaches the pty");

        assert!(
            wait_until(10, || probe.output.lock().unwrap().contains("seam:ok")),
            "command output crossed the callback seam; saw: {:?}",
            probe.output.lock().unwrap()
        );
        assert!(
            wait_until(10, || probe.exit_for(started.pid).is_some()),
            "the shell's own exit emitted on_closed"
        );
        let exit = probe.exit_for(started.pid).expect("closed entry");
        assert_eq!(
            exit.exit_code, 7,
            "the widened payload carries the shell's own exit code"
        );
        assert!(exit.signal.is_none(), "a natural exit names no signal");
        assert!(
            write_terminal(started.pid, "x".to_string()).is_err(),
            "a closed session's pid no longer accepts writes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `close_terminal` kills the child, and the reader thread still delivers
    /// the same `on_closed` as a natural exit — the one end-of-session signal
    /// the UI keys off in both cases.
    #[test]
    fn close_terminal_kills_the_session_and_retires_the_pid() {
        let dir = std::env::temp_dir().join(format!("leogit-ffi-term-kill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let probe = Arc::new(TerminalProbe::default());
        let started = start_terminal(
            probe.clone(),
            dir.to_string_lossy().to_string(),
            Some("sh".to_string()),
        )
        .expect("terminal starts");

        resize_terminal(started.pid, 120, 40).expect("a live session resizes");
        close_terminal(started.pid).expect("close succeeds");

        assert!(
            wait_until(10, || probe.exit_for(started.pid).is_some()),
            "a kill still ends in on_closed"
        );
        let exit = probe.exit_for(started.pid).expect("closed entry");
        assert!(
            exit.signal.is_some() || exit.exit_code != 0,
            "a killed shell must not report a clean exit: {exit:?}"
        );
        assert!(
            resize_terminal(started.pid, 80, 24).is_err(),
            "the killed session's pid is retired"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Clone flow end-to-end against a local source: the export returns
    /// the destination it was given, the clone is a real repository, and the
    /// same destination is refused on a second attempt (core's
    /// `prepare_clone_target` guard) — the error the sheet must surface.
    #[tokio::test]
    async fn clone_repo_clones_a_local_source_and_refuses_an_existing_target() {
        let base = std::env::temp_dir().join(format!("leogit-ffi-clone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let source = base.join("source");
        std::fs::create_dir_all(&source).expect("source dir");
        run_git(&source, &["init"]);
        std::fs::write(source.join("a.txt"), "a\n").expect("write");
        run_git(&source, &["add", "."]);
        run_git(
            &source,
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

        let target = base.join("cloned");
        let listener = CollectingListener::arc();
        let cloned = clone_repo(
            source.to_string_lossy().to_string(),
            target.to_string_lossy().to_string(),
            listener.clone(),
        )
        .await
        .expect("local clone succeeds");

        assert_eq!(cloned, target.to_string_lossy(), "returns the destination");
        assert!(
            Path::new(&cloned).join(".git").exists(),
            "the clone is a git repository"
        );
        assert!(
            std::fs::read_to_string(target.join("a.txt")).is_ok(),
            "the worktree is checked out"
        );
        // A tiny local clone may finish inside git's progress throttle, so
        // tick *presence* is not asserted — only that any that did arrive
        // carry sane aggregate percents.
        assert!(
            listener
                .events
                .lock()
                .expect("lock")
                .iter()
                .all(|tick| (0.0..=100.0).contains(&tick.percent)),
            "progress percents stay within 0–100"
        );

        let err = clone_repo(
            source.to_string_lossy().to_string(),
            target.to_string_lossy().to_string(),
            CollectingListener::arc(),
        )
        .await
        .expect_err("an existing destination is refused");
        let GitError::Failed { message } = err;
        assert!(
            message.contains("already exists"),
            "guard fires before git: {message}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// `gh_clone` shares `clone_repo`'s destination guard, and it fires
    /// before `gh` is ever spawned — so this pins the contract without
    /// needing the GitHub CLI installed or authenticated.
    #[tokio::test]
    async fn gh_clone_refuses_an_existing_target_before_running_gh() {
        let target =
            std::env::temp_dir().join(format!("leogit-ffi-ghclone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).expect("target dir");

        let err = gh_clone(
            CollectingListener::arc(),
            "owner/repo".to_string(),
            target.to_string_lossy().to_string(),
        )
        .await
        .expect_err("an existing destination is refused");
        let GitError::Failed { message } = err;
        assert!(
            message.contains("already exists"),
            "guard fires before gh: {message}"
        );

        let _ = std::fs::remove_dir_all(&target);
    }

    /// `gh_publish_repo`'s empty-name guard fires before `gh` is spawned —
    /// the one binary-free assertion the publish flow offers (the repo path
    /// is deliberately not validated; `gh` itself reports everything else).
    /// The whitespace name also pins that the *trimmed* name is what counts.
    #[tokio::test]
    async fn gh_publish_repo_requires_a_name_before_running_gh() {
        let err = gh_publish_repo(
            "/nonexistent".to_string(),
            "   ".to_string(),
            String::new(),
            true,
        )
        .await
        .expect_err("a blank name is refused");
        let GitError::Failed { message } = err;
        assert_eq!(message, "Repository name is required.");
    }

    /// The Settings shell picker's data source: probe-based, so every row it
    /// offers is actually launchable on this machine, and never empty.
    #[test]
    fn list_shells_offers_only_launchable_shells() {
        let shells = list_shells();
        assert!(!shells.is_empty(), "at least the fallback shell exists");
        for shell in &shells {
            assert!(!shell.id.is_empty(), "every row has a stable id");
            assert!(!shell.label.is_empty(), "every row has a label");
            assert!(
                Path::new(&shell.path).exists(),
                "{}: probed path exists on disk",
                shell.id
            );
        }
    }
}

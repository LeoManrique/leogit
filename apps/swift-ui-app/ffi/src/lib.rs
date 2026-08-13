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
//! (`fetch` / `pull` / `push`) are exported async instead — core drives them
//! through `spawn_blocking`, which needs a live tokio context, hence
//! `async_runtime = "tokio"` — and surface in Swift as native `async` calls.
//! Progress callbacks arrive on core's stderr-reader thread, never the main
//! one; Swift listeners must hop to the main actor themselves.

// Exported signatures are dictated by UniFFI, not by Rust ergonomics: arguments
// arrive as owned values across the FFI boundary, so taking `String` by value is
// required rather than avoidable.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;
use std::sync::Arc;

use leogit_core::events::{CoreEvent, EventSink};
use leogit_core::{ai, config, diff, git, highlight, process};

// Re-exported so Swift sees the real core types. Names are used by the
// `#[uniffi::remote]` declarations below.
pub use leogit_core::ai::{AiProviderConfig, CommitMessage};
pub use leogit_core::diff::{DiffLine, FileDiff, Hunk, HunkHeader, IntraLineRange, LineType};
pub use leogit_core::git::{
    BranchInfo, CommitInfo, FileEntry, FileStatus, LogOptions, MergeResult, RepoStatus,
};
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

/// Whether a merge is in progress (`MERGE_HEAD` exists) — drives the
/// "merging" badge and the Abort Merge affordance. Worktree-safe.
///
/// # Errors
///
/// Never fails in practice; an unreadable repository reports `false`.
#[uniffi::export]
pub fn is_merging(repo_path: String) -> Result<bool, GitError> {
    git::is_merging(repo_path).map_err(GitError::from)
}

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
// Exported functions — sync (fetch / pull / push)
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
/// to `pull`/`push`, which emit nothing else.
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
pub fn get_remote(repo_path: String) -> Result<String, GitError> {
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
pub async fn fetch(repo_path: String, remote: String) -> Result<(), GitError> {
    git::fetch(repo_path, remote).await.map_err(GitError::from)
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
// `check_provider_available` stays unexported: it has no callers in either
// client (the Tauri API wrapper for it is dead code).

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
/// as provider knobs only (`model`, `base_url`; `api_key` is accepted for
/// wire-compatibility but read by neither provider). `None` fields fall back
/// inside core: model `"sonnet"` (claude) / `"tavernari/git-commit-message:
/// latest"` (ollama), base URL `http://localhost:11434`.
#[uniffi::remote(Record)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// The config→provider mapping the Tauri client performs in TypeScript
/// (`CommitMessage.svelte`) before every generate call, ported verbatim so
/// the two clients can never drift on which config field feeds which knob.
fn ai_provider_config(cfg: config::Config) -> AiProviderConfig {
    AiProviderConfig {
        // Anything unrecognized falls back to claude, matching the
        // frontend's `=== 'ollama' ? 'ollama' : 'claude'` guard.
        provider: if cfg.ai_provider == "ollama" {
            "ollama".to_string()
        } else {
            "claude".to_string()
        },
        model: cfg.ai_model,
        api_key: cfg.ai_api_key,
        base_url: Some(cfg.ollama_server_url),
    }
}

/// The AI settings from the shared config file, ready to pass to
/// [`generate_commit_message`]. Loaded fresh before every generate — never
/// cached — so an edit to `config.toml` (or a save from the Tauri client)
/// takes effect on the next click.
///
/// # Errors
///
/// Returns [`GitError`] when the config file exists but cannot be read or
/// parsed. A missing file is not an error — defaults are written and used.
#[uniffi::export]
pub fn load_ai_config() -> Result<AiProviderConfig, GitError> {
    config::load_config()
        .map(ai_provider_config)
        .map_err(GitError::from)
}

/// Persist the composer's provider choice (`"claude"` | `"ollama"`) into the
/// shared config file — a read-modify-write of the whole `Config`, so every
/// other setting survives untouched. Validated before touching disk: an
/// unknown value can never be persisted.
///
/// # Errors
///
/// Returns [`GitError`] for an unknown provider name, or when the config
/// file cannot be read or written.
#[uniffi::export]
pub fn save_ai_provider(provider: String) -> Result<(), GitError> {
    if !matches!(provider.as_str(), "claude" | "ollama") {
        return Err(GitError::Failed {
            message: format!("Unknown AI provider: {provider}"),
        });
    }
    let mut cfg = config::load_config().map_err(GitError::from)?;
    cfg.ai_provider = provider;
    config::save_config(cfg).map_err(GitError::from)
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
    /// (`success == false` + conflicted paths), flip `is_merging`, and clean
    /// up via `merge_abort`.
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
            is_merging(repo.clone()).expect("is_merging"),
            "MERGE_HEAD exists"
        );

        merge_abort(repo.clone()).expect("abort");
        assert!(
            !is_merging(repo).expect("is_merging"),
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
            "origin",
            "no remote configured falls back to the literal origin"
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
        fetch(repo.clone(), "origin".to_string())
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
        fetch(repo.clone(), "origin".to_string())
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

    /// The bridge's config→provider mapping — the port of what the Tauri
    /// client does in TypeScript before every generate call. Pinned so the
    /// two clients can't drift on which config field feeds which knob.
    #[test]
    fn ai_config_mapping_matches_the_tauri_client() {
        let defaults = ai_provider_config(config::Config::default());
        assert_eq!(defaults.provider, "claude");
        assert_eq!(defaults.model, None);
        assert_eq!(
            defaults.base_url.as_deref(),
            Some("http://localhost:11434"),
            "the ollama URL always travels as base_url, like the frontend"
        );

        let ollama = ai_provider_config(config::Config {
            ai_provider: "ollama".to_string(),
            ai_model: Some("llama3".to_string()),
            ..config::Config::default()
        });
        assert_eq!(ollama.provider, "ollama");
        assert_eq!(ollama.model.as_deref(), Some("llama3"));

        let unknown = ai_provider_config(config::Config {
            ai_provider: "copilot".to_string(),
            ..config::Config::default()
        });
        assert_eq!(
            unknown.provider, "claude",
            "unrecognized providers fall back to claude"
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
            api_key: None,
            base_url: None,
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

    /// `save_ai_provider` validates before touching the config file, so a bad
    /// value can never be persisted — which is also what keeps this test away
    /// from the user's real `config.toml`.
    #[test]
    fn save_ai_provider_rejects_unknown_providers() {
        let err = save_ai_provider("copilot".to_string()).unwrap_err();
        assert!(
            matches!(err, GitError::Failed { ref message } if message.contains("Unknown AI provider"))
        );
    }
}

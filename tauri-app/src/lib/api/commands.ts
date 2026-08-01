import { invoke } from '@tauri-apps/api/core'

export interface FileEntry {
  path: string
  orig_path?: string
  status: 'New' | 'Modified' | 'Deleted' | 'Renamed' | 'Conflicted'
  xy: string
  display_name: string
  display_dir: string
  /**
   * True when this entry is an embedded git repository (a nested repo with its
   * own `.git`). It commits as a gitlink — a pointer to the nested repo's
   * commit — rather than copying the folder's files, so the UI flags the row
   * and confirms before committing.
   */
  embedded: boolean
  /**
   * True when this entry is a tracked submodule that is dirty inside (its own
   * working tree has modified/untracked content) but whose recorded commit
   * pointer has NOT moved. The parent repo has nothing to stage, so a commit
   * would fail — the inner changes must be committed inside the submodule
   * first. The UI disables the row instead of letting the commit fail.
   */
  submodule_dirty: boolean
}

export interface RepoStatus {
  branch: string
  upstream: string
  has_upstream: boolean
  ahead: number
  behind: number
  files: FileEntry[]
  /** Whether the repo has any configured remote. Drives Push vs. Publish in the UI. */
  has_remote: boolean
  /** SHAs of commits reachable from HEAD but not on the remote. Empty when in sync or no upstream. */
  unpushed_shas: string[]
  /** True when HEAD is detached (on a commit, not a branch), e.g. after "Checkout commit". */
  detached: boolean
  /** Full SHA of HEAD; empty only on an unborn branch. Labels the detached-HEAD state. */
  head_sha: string
}

export interface CommitInfo {
  sha: string
  short_sha: string
  summary: string
  body: string
  author_name: string
  author_email: string
  author_date: string
  committer_name: string
  committer_date: string
  parents: string[]
  trailers: string[]
  refs: string[]
}

/** Aggregate line-change totals for a commit, summed across all its files. */
export interface CommitStats {
  additions: number
  deletions: number
}

export interface BranchInfo {
  name: string
  is_remote: boolean
  is_current: boolean
}

export interface MergeResult {
  success: boolean
  fast_forward: boolean
  conflicts: string[]
  error_message?: string
}

export interface AheadBehind {
  ahead: number
  behind: number
}

/** Lightweight per-repo sync summary for the picker's pull/push badges. */
export interface RepoSync {
  ahead: number
  behind: number
  has_remote: boolean
  /** Whether a requested background fetch actually reached the remote (true
   * when none was requested / no remote). Feeds the connectivity breaker. */
  fetched: boolean
}

export interface RepoIdentifier {
  owner: string
  name: string
}

export interface IntraLineRange {
  start: number
  length: number
}

export interface DiffLine {
  text: string
  content: string
  line_type: 'Context' | 'Add' | 'Delete' | 'Hunk' | 'NoNewline'
  old_line_no?: number
  new_line_no?: number
  intra_line_diff?: IntraLineRange | null
}

export interface HunkHeader {
  old_start: number
  old_count: number
  new_start: number
  new_count: number
}

export interface Hunk {
  header: HunkHeader
  lines: DiffLine[]
}

export interface FileDiff {
  old_path: string
  new_path: string
  file_header: string
  hunks: Hunk[]
  is_binary: boolean
}

export interface DiffSelection {
  default_selected: boolean
  diverging_lines: Record<number, boolean>
}

export interface CommitMessage {
  title: string
  description: string
}

export interface Config {
  theme: string
  fetch_interval_ms: number
  ai_provider: string
  ai_model?: string
  ai_api_key?: string
  auto_fetch: boolean
  syntax_highlighting: boolean
  scan_paths: string[]
  scan_depth: number
  side_by_side_diff: boolean
  hide_whitespace: boolean
  wrap_long_lines: boolean
  tab_size: number
  claude_timeout_secs: number
  ollama_server_url: string
}

export interface ReposState {
  last_opened_repo?: string
  /** Parent folder of the last clone; pre-fills the Clone dialog destination. */
  last_clone_dir?: string
  /** Persisted sort mode for the repo picker ('recent' | 'name'). */
  repo_sort_mode?: string
  /** Persisted sort mode for the Clone dialog's GitHub repo list ('recent' | 'name'). */
  clone_sort_mode?: string
  /** Repo paths, most-recently-opened first. Drives the picker's tiered sync. */
  recent_repos?: string[]
}

/** A repository surfaced in the GitHub tab of the Clone dialog (`gh repo list`). */
export interface GhRepo {
  name_with_owner: string
  name: string
  description: string
  is_private: boolean
  /** ISO-8601 last-push timestamp; used to sort by "recently modified". */
  pushed_at: string
}

export const configApi = {
  loadConfig: () => invoke<Config>('load_config'),
  saveConfig: (cfg: Config) => invoke<void>('save_config', { cfg }),
  loadState: () => invoke<ReposState>('load_state'),
  saveState: (state: ReposState) => invoke<void>('save_state', { state }),
}

/**
 * Payload of the `git-progress` event: live transfer progress emitted by the
 * backend while a push, pull, or clone streams git's `--progress` output.
 */
export interface GitProgressEvent {
  op: 'push' | 'pull' | 'clone'
  /** Repo path the op runs against (the target folder for a clone). */
  path: string
  /** Aggregate 0–100 across the operation's phases. */
  percent: number
  /** Raw git progress line, e.g. "Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s". */
  text: string
}

/** Where a `leogit <dir>` invocation points. Payload of the `open-repo` event. */
export interface LaunchTarget {
  /** Absolute path — the repository root when `is_repo`, else the folder itself. */
  path: string
  /**
   * False when the folder exists but isn't inside a git repository. The app
   * offers to create one there rather than opening it.
   */
  is_repo: boolean
}

export const appApi = {
  /**
   * Claim the folder passed on a cold-start `leogit <dir>` command line (null
   * for a bare launch). Warm starts arrive via the `open-repo` event instead.
   */
  takePendingLaunchTarget: () => invoke<LaunchTarget | null>('take_pending_launch_target'),
}

export const gitApi = {
  getStatus: (repoPath: string) => invoke<RepoStatus>('get_status', { repoPath }),
  getHeadSha: (repoPath: string) => invoke<string>('get_head_sha', { repoPath }),
  getDiff: (repoPath: string, file: FileEntry) => invoke<string>('get_diff', { repoPath, file }),
  getDiffWhitespaceIgnored: (repoPath: string, file: FileEntry) =>
    invoke<string>('get_diff_whitespace_ignored', { repoPath, file }),
  getCommitDiff: (repoPath: string, sha: string, filePath: string) =>
    invoke<string>('get_commit_diff', { repoPath, sha, filePath }),
  getSelectedDiff: (repoPath: string, files: FileEntry[]) =>
    invoke<string>('get_selected_diff', { repoPath, files }),
  getLog: (repoPath: string, maxCount: number, skip: number) =>
    invoke<CommitInfo[]>('get_log', { repoPath, opts: { max_count: maxCount, skip } }),
  getCommitFiles: (repoPath: string, sha: string) =>
    invoke<FileEntry[]>('get_commit_files', { repoPath, sha }),
  getCommitStats: (repoPath: string, sha: string) =>
    invoke<CommitStats>('get_commit_stats', { repoPath, sha }),
  listBranches: (repoPath: string) => invoke<BranchInfo[]>('list_branches', { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    invoke<void>('create_branch', { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, branch: string) =>
    invoke<void>('switch_branch', { repoPath, branch }),
  /** Check out a commit by SHA, detaching HEAD. Mirrors GitHub Desktop's "Checkout commit". */
  checkoutCommit: (repoPath: string, sha: string) =>
    invoke<void>('checkout_commit', { repoPath, sha }),
  deleteBranch: (repoPath: string, name: string) =>
    invoke<void>('delete_branch', { repoPath, name }),
  deleteRemoteBranch: (repoPath: string, remote: string, branch: string) =>
    invoke<void>('delete_remote_branch', { repoPath, remote, branch }),
  renameBranch: (repoPath: string, oldName: string, newName: string) =>
    invoke<void>('rename_branch', { repoPath, oldName, newName }),
  commit: (repoPath: string, message: string, files: FileEntry[], amend: boolean = false) =>
    invoke<void>('commit', { repoPath, message, files, amend }),
  undoLastCommit: (repoPath: string) => invoke<void>('undo_last_commit', { repoPath }),
  hasStagedChanges: (repoPath: string) => invoke<boolean>('has_staged_changes', { repoPath }),
  /** Discard working-tree changes for the given files (revert tracked, trash untracked). */
  discardFiles: (repoPath: string, files: FileEntry[]) =>
    invoke<void>('discard_files', { repoPath, files }),
  /** Append the given ready-to-write patterns to the repo's root .gitignore. */
  appendToGitignore: (repoPath: string, patterns: string[]) =>
    invoke<void>('append_to_gitignore', { repoPath, patterns }),
  formatCommitMessage: (summary: string, description: string, coAuthors: string[] = []) =>
    invoke<string>('format_commit_message', { summary, description, coAuthors }),
  repoSyncStatus: (repoPath: string, doFetch: boolean) =>
    invoke<RepoSync>('repo_sync_status', { repoPath, doFetch }),
  fetch: (repoPath: string, remote: string) => invoke<void>('fetch', { repoPath, remote }),
  pull: (repoPath: string, remote: string) => invoke<void>('pull', { repoPath, remote }),
  push: (
    repoPath: string,
    remote: string,
    branch: string,
    setUpstream: boolean,
    forceWithLease: boolean
  ) => invoke<void>('push', { repoPath, remote, branch, setUpstream, forceWithLease }),
  getAheadBehind: (repoPath: string, upstream: string) =>
    invoke<AheadBehind>('get_ahead_behind', { repoPath, upstream }),
  getRemote: (repoPath: string) => invoke<string>('get_remote', { repoPath }),
  getRepoIdentifier: (repoPath: string) =>
    invoke<RepoIdentifier | null>('get_repo_identifier', { repoPath }),
  mergeBranch: (repoPath: string, branch: string) =>
    invoke<MergeResult>('merge_branch', { repoPath, branch }),
  mergeSquash: (repoPath: string, branch: string) =>
    invoke<MergeResult>('merge_squash', { repoPath, branch }),
  commitSquashMerge: (repoPath: string) => invoke<void>('commit_squash_merge', { repoPath }),
  mergeAbort: (repoPath: string) => invoke<void>('merge_abort', { repoPath }),
  isMerging: (repoPath: string) => invoke<boolean>('is_merging', { repoPath }),
  countCommitsToMerge: (repoPath: string, targetBranch: string) =>
    invoke<number>('count_commits_to_merge', { repoPath, targetBranch }),
  discoverRepos: (scanPaths: string[], maxDepth: number) =>
    invoke<string[]>('discover_repos', { scanPaths, maxDepth }),
  isGitRepo: (path: string) => invoke<boolean>('is_git_repo', { path }),
  /**
   * `git init` a folder so it can be opened, returning the path to open.
   * Idempotent — a folder already inside a repo returns that repo's root.
   */
  initRepo: (path: string) => invoke<string>('init_repo', { path }),
  getRepoName: (path: string) => invoke<string>('get_repo_name', { path }),
  cloneRepo: (url: string, targetPath: string) => invoke<string>('clone_repo', { url, targetPath }),
  getLastCommitTimestamp: (repoPath: string) =>
    invoke<number>('get_last_commit_timestamp', { repoPath }),
}

/**
 * OS-shell integration for working-tree files. Paths are repo-relative; the
 * backend joins them onto the repo path so a Windows backslash vs. git's
 * forward-slash mismatch never reaches the file system.
 */
export const osApi = {
  /** Reveal a file in the platform file manager (Finder / Explorer / file manager). */
  revealPath: (repoPath: string, relPath: string) =>
    invoke<void>('reveal_path', { repoPath, relPath }),
  /** Open a file with the OS's default application for its type. */
  openPath: (repoPath: string, relPath: string) => invoke<void>('open_path', { repoPath, relPath }),
}

export const diffApi = {
  parseDiff: (raw: string) => invoke<FileDiff | null>('parse_diff', { raw }),
  generatePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_patch', { repoPath, fileDiff, selection }),
  generateInversePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_inverse_patch', { repoPath, fileDiff, selection }),
}

/**
 * Token classes emitted by the Rust syntect tokenizer (`highlight_diff`).
 * The numeric values MUST stay in sync with `TokenClass` in
 * `src-tauri/src/commands/highlight.rs` — Rust serialises the enum as its
 * `#[repr(u8)]` index, so re-ordering breaks the wire format.
 */
export const TokenClass = {
  Plain: 0,
  Keyword: 1,
  String: 2,
  Comment: 3,
  Function: 4,
  Type: 5,
  Variable: 6,
  Number: 7,
  Constant: 8,
  Operator: 9,
  Punctuation: 10,
  Tag: 11,
  Attribute: 12,
  Builtin: 13,
  Decorator: 14,
  // Markup / prose classes (Markdown, reStructuredText, …).
  Heading: 15,
  Strong: 16,
  Emphasis: 17,
  Strikethrough: 18,
  Link: 19,
  Raw: 20,
  Quote: 21,
} as const

export type TokenClassValue = (typeof TokenClass)[keyof typeof TokenClass]

export interface Token {
  /** Code-point index into the line `content` (matches `IntraLineRange`). */
  start: number
  /** Code-point index (exclusive) into the line `content`. */
  end: number
  class: TokenClassValue
}

export type TokenLine = Token[]

/**
 * Where the highlighter should read the diff's old/new sides from. syntect is a
 * stateful, line-sequential parser, so it has to read each side's full blob from
 * line 1 — the diff's own lines never establish which context a line sits in
 * (e.g. inside a `<script lang="ts">` block). Mirrors the two views that produce
 * a diff; Rust owns the rev-spec details.
 */
export type BlobSource =
  /** Uncommitted changes: old side is HEAD, new side is the working tree. */
  | { kind: 'workingTree'; repoPath: string }
  /** A committed diff: old side is the commit's first parent, new side the commit. */
  | { kind: 'commit'; repoPath: string; sha: string }

export const highlightApi = {
  /** `source` omitted falls back to a diff-only parse, which is only correct
   *  when the first hunk starts in the file's top-level context. */
  highlightDiff: (fileDiff: FileDiff, source?: BlobSource | null) =>
    invoke<TokenLine[]>('highlight_diff', { fileDiff, source: source ?? null }),
}

export const ghApi = {
  checkAuth: () => invoke<boolean>('check_auth'),
  repoList: (limit: number) => invoke<GhRepo[]>('gh_repo_list', { limit }),
  clone: (nameWithOwner: string, targetPath: string) =>
    invoke<string>('gh_clone', { nameWithOwner, targetPath }),
  publishRepo: (repoPath: string, name: string, description: string, isPrivate: boolean) =>
    invoke<void>('gh_publish_repo', { repoPath, name, description, isPrivate }),
}

export interface AiProviderConfig {
  provider: string
  model?: string
  api_key?: string
  base_url?: string
}

export const aiApi = {
  generateCommitMessage: (diff: string, provider: string, config: AiProviderConfig) =>
    invoke<CommitMessage>('generate_commit_message', { diff, provider, config }),
  checkProviderAvailable: (provider: string, config: AiProviderConfig) =>
    invoke<boolean>('check_provider_available', { provider, config }),
}

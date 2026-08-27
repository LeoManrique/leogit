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
  /**
   * Opaque content-change stamp (mtime + size) for the working-tree side, so
   * a status comparison can see content edits; absent when nothing is on disk
   * (deletions) and in commit-file lists. This client doesn't read it yet —
   * the native client keys its open-diff reload on it. Compare, never parse.
   */
  stat_stamp?: string
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
  /** Values of `Co-Authored-By:` trailers (e.g. "Jane <jane@x.com>"), pre-parsed
   * by the backend for re-application on amend / undo-commit restore. */
  co_authors: string[]
  /** `body` with its `Co-Authored-By:` lines removed — what the composer
   * pre-fills, since co-authors are re-applied via `format_commit_message`. */
  body_without_coauthors: string
  /** Names of tags pointing at this commit (from `%D`, `tag: ` prefix stripped). */
  tags: string[]
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
  /** Whether the working tree has uncommitted changes (what the Changes tab
   * would list). Drives the picker's dirty dot. */
  dirty: boolean
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

/**
 * One row of the side-by-side layout: old-side and new-side lines referenced
 * by flat/global index into the hunks' concatenated `lines` arrays (the same
 * indexing the per-line HTML and the selection map use). `null` renders an
 * empty filler cell.
 */
export interface SbsPair {
  left: number | null
  right: number | null
  is_hunk_header: boolean
}

/**
 * Everything the viewer needs from one `parse_diff` round trip. `file_diff`
 * stays lean because it round-trips back into `highlight_diff` /
 * `generate_patch`; the derived render artifacts ride alongside.
 */
export interface ParsedDiff {
  file_diff: FileDiff
  /** Phase-1 HTML per flattened line (plain escaped text + intra-line
   * backplate), ready for `{@html}` the same frame the diff mounts. */
  html: string[]
  /** Precomputed rows for the side-by-side layout. */
  sbs_pairs: SbsPair[]
  /** Added-line total for the header badge (0 for binary diffs). */
  additions: number
  /** Deleted-line total for the header badge (0 for binary diffs). */
  deletions: number
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
  /** Shell id the embedded terminal launches; absent = best available. */
  terminal_shell?: string
}

export interface ReposState {
  last_opened_repo?: string
  /** Parent folder of the last clone; pre-fills the Clone dialog destination. */
  last_clone_dir?: string
  /** Persisted sort mode for the repo picker ('recent' | 'name'). */
  repo_sort_mode?: string
  /** Persisted sort mode for the Clone dialog's GitHub repo list ('recent' | 'name'). */
  clone_sort_mode?: string
  /** Repo paths, most-recently-opened first. Drives the picker's tiered sync.
   * Owned by the backend's `record_recent_repo` (de-dupes, caps at 50). */
  recent_repos?: string[]
}

/**
 * Field-wise patch for `ReposState`: absent fields are left as they are on
 * disk. The backend applies it as one atomic read-modify-write under a lock.
 * `recent_repos` is deliberately not patchable — `record_recent_repo` is the
 * MRU list's only writer.
 */
export interface ReposStatePatch {
  last_opened_repo?: string
  last_clone_dir?: string
  repo_sort_mode?: string
  clone_sort_mode?: string
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
  /** Atomically merge the given fields into repos-state.json; returns the new state. */
  patchState: (patch: ReposStatePatch) => invoke<ReposState>('patch_state', { patch }),
  /** Move a repo to the front of the persisted MRU list; returns the new state,
   * whose `recent_repos` is the authoritative list to reseed the store from. */
  recordRecentRepo: (path: string) => invoke<ReposState>('record_recent_repo', { path }),
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
  /** Append ready-to-write glob patterns (e.g. `*.log`) to the repo's root .gitignore. */
  appendToGitignore: (repoPath: string, patterns: string[]) =>
    invoke<void>('append_to_gitignore', { repoPath, patterns }),
  /** Ignore literal file paths — the backend escapes their glob metacharacters. */
  ignorePaths: (repoPath: string, paths: string[]) =>
    invoke<void>('ignore_paths', { repoPath, paths }),
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
  /**
   * The folders `discoverRepos` would actually walk for this config — the
   * configured list, or the stock defaults when it's empty. Lets the picker's
   * empty state name where it searched instead of just saying "none found".
   */
  effectiveScanPaths: (scanPaths: string[]) =>
    invoke<string[]>('effective_scan_paths', { scanPaths }),
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
 * OS hand-off commands. File paths are repo-relative; the backend joins them
 * onto the repo path so a Windows backslash vs. git's forward-slash mismatch
 * never reaches the file system.
 */
export const osApi = {
  /** Reveal a file in the platform file manager (Finder / Explorer / file manager). */
  revealPath: (repoPath: string, relPath: string) =>
    invoke<void>('reveal_path', { repoPath, relPath }),
  /** Open a file with the OS's default application for its type. */
  openPath: (repoPath: string, relPath: string) => invoke<void>('open_path', { repoPath, relPath }),
  /** Open an https:// URL in the default browser. */
  openUrl: (url: string) => invoke<void>('open_url', { url }),
}

/** A shell the embedded terminal can launch, as probed on this machine. */
export interface ShellOption {
  /** Stable id persisted in `Config.terminal_shell` (e.g. `git-bash`). */
  id: string
  /** Human-readable name for the picker. */
  label: string
  /** Absolute path to the executable. */
  path: string
  args: string[]
}

/**
 * PTY backend description, needed to configure xterm.js *before* it builds a
 * buffer. Telling xterm it's on ConPTY with a build >= 21376 enables reflow on
 * resize; without it xterm guesses that any line whose last cell is non-blank
 * is wrapped, which is what smears a resized prompt on Windows.
 */
export interface PtyInfo {
  /** `'conpty'` on Windows, `null` elsewhere. */
  backend: string | null
  build_number: number | null
}

/** Result of starting a session: the handle, plus the shell actually launched. */
export interface StartedTerminal {
  pid: number
  shell_id: string
  shell_label: string
}

export const terminalApi = {
  /** Shells launchable on this machine, best-first. Never empty. */
  listShells: () => invoke<ShellOption[]>('list_shells'),
  /** Describe the PTY backend; call before constructing the xterm instance. */
  ptyInfo: () => invoke<PtyInfo>('terminal_pty_info'),
  /** Start a shell with cwd=repoPath. `shellId` absent = best available. */
  start: (repoPath: string, shellId?: string) =>
    invoke<StartedTerminal>('start_terminal', { repoPath, shellId: shellId ?? null }),
  write: (pid: number, data: string) => invoke<void>('write_terminal', { pid, data }),
  resize: (pid: number, cols: number, rows: number) =>
    invoke<void>('resize_terminal', { pid, cols, rows }),
  close: (pid: number) => invoke<void>('close_terminal', { pid }),
}

export const diffApi = {
  parseDiff: (raw: string) => invoke<ParsedDiff | null>('parse_diff', { raw }),
  generatePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_patch', { repoPath, fileDiff, selection }),
  generateInversePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_inverse_patch', { repoPath, fileDiff, selection }),
}

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
  /** Returns render-ready HTML per flattened diff line — syntect token spans
   *  laid over the same intra-line backplate as `ParsedDiff.html`, replacing
   *  it wholesale. `source` omitted falls back to a diff-only parse, which is
   *  only correct when the first hunk starts in the file's top-level context. */
  highlightDiff: (fileDiff: FileDiff, source?: BlobSource | null) =>
    invoke<string[]>('highlight_diff', { fileDiff, source: source ?? null }),
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

/** A newer leogit release on GitHub, as reported by `check_for_update`. */
export interface UpdateInfo {
  /** Latest released version, without the leading `v`. */
  version: string
  /** The GitHub release page (download assets + notes). */
  url: string
  /** Terminal one-liner that upgrades in place; null on Windows, where the
   * release-page download is the path instead. */
  install_command: string | null
}

export const updateApi = {
  /** Ask GitHub Releases for a version newer than this build; null when
   * current. Rejects when the check itself fails, so callers can retry. */
  checkForUpdate: () => invoke<UpdateInfo | null>('check_for_update'),
}

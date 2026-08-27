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
  /**
   * Whether a merge is in progress (`MERGE_HEAD` exists). Carried here rather
   * than fetched separately: every refresh path needs it, one of them used to
   * forget, and core answers it from a file check that costs the poll nothing.
   */
  merging: boolean
}

/** One status's presentation strings — see `fileStatusStyles`. */
export interface FileStatusStyle {
  status: FileEntry['status']
  letter: string
  label: string
}

/** The files a commit changed, plus its line totals — one `git log`. */
export interface CommitDetail {
  files: FileEntry[]
  stats: CommitStats
}

/**
 * What discarding a set of files would do, path by path. A row's status letter
 * can't answer this — a staged re-add of a path that exists in HEAD is
 * restorable, and under an unborn HEAD nothing is — so the dialog is told.
 */
export interface DiscardPlan {
  /** Paths restored to their committed state (index and working tree). */
  restore: string[]
  /** Paths with no committed version: moved to the Trash, unstaged. */
  trash: string[]
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
  /**
   * The raw patch line, prefix included — present only where something reads
   * it: a `Hunk` header and a `NoNewline` marker, whose whole meaning is
   * their text. Everywhere else it duplicated `content` byte for byte, once
   * per line of every diff.
   */
  text?: string
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

/** Why a diff has no lines to show. */
export type EmptyDiffReason = 'NoChanges' | 'WhitespaceOnly' | 'NoTextualChanges'

/** Which size limit withheld a diff. */
export type DiffSizeReason = 'TotalBytes' | 'LineLength'

/** A diff too large to render eagerly, and the measurements that say so. */
export interface DiffSizeGuard {
  reason: DiffSizeReason
  /** Size of the raw patch in bytes — what the message quotes. */
  bytes: number
  /** Longest single line, in bytes. */
  longest_line: number
}

/**
 * What the caller wants built alongside the parse. This client paints from
 * `html` on the first frame and pairs rows from `sbs_pairs` in the split
 * layout, so it asks for both; the native client renders from the line model
 * and asks for neither.
 */
export interface DiffOptions {
  html: boolean
  side_by_side: boolean
  /** Parse past the size guard — the viewer's "Show diff anyway". */
  show_anyway: boolean
}

/**
 * Everything the viewer needs from one round trip. `file_diff` stays lean
 * because it round-trips back into `highlight_diff` / `generate_patch`; the
 * derived render artifacts ride alongside.
 */
export interface ParsedDiff {
  file_diff: FileDiff
  /** Phase-1 HTML per flattened line (plain escaped text + intra-line
   * backplate), ready for `{@html}` the same frame the diff mounts. Empty
   * unless `DiffOptions.html` asked for it. */
  html: string[]
  /** Precomputed rows for the side-by-side layout. Empty unless
   * `DiffOptions.side_by_side` asked for it. */
  sbs_pairs: SbsPair[]
  /** Added-line total for the header badge (0 for binary diffs). */
  additions: number
  /** Deleted-line total for the header badge (0 for binary diffs). */
  deletions: number
  /** Set when there are nothing to show, and why. */
  empty_reason?: EmptyDiffReason | null
  /** Set when the diff was withheld for its size. */
  size_guard?: DiffSizeGuard | null
}

export interface DiffSelection {
  default_selected: boolean
  diverging_lines: Record<number, boolean>
}

export interface CommitMessage {
  title: string
  description: string
}

/** Claude-specific AI settings. Each provider keeps its own model. */
export interface ClaudeConfig {
  model?: string
  timeout_secs: number
}

/** Ollama-specific AI settings. */
export interface OllamaConfig {
  model?: string
  server_url: string
  timeout_secs: number
}

export interface Config {
  theme: string
  fetch_interval_ms: number
  ai_provider: string
  auto_fetch: boolean
  syntax_highlighting: boolean
  scan_paths: string[]
  scan_depth: number
  side_by_side_diff: boolean
  hide_whitespace: boolean
  tab_size: number
  /** Shell id the embedded terminal launches; absent = best available. */
  terminal_shell?: string
  claude: ClaudeConfig
  ollama: OllamaConfig
}

/**
 * Field-wise patch for `Config`: an absent field is left as it is on disk.
 *
 * The only writer. Two clients share this file, and the whole-object write
 * this replaces posted the config as it looked when a dialog *opened*,
 * silently reverting whatever the other client had written since. Clearing an
 * optional field is patching it to `''` — the config's standing
 * blank-means-absent rule.
 */
export interface ConfigPatch {
  theme?: string
  fetch_interval_ms?: number
  ai_provider?: string
  auto_fetch?: boolean
  syntax_highlighting?: boolean
  scan_paths?: string[]
  scan_depth?: number
  side_by_side_diff?: boolean
  hide_whitespace?: boolean
  tab_size?: number
  terminal_shell?: string
  claude_model?: string
  claude_timeout_secs?: number
  ollama_model?: string
  ollama_server_url?: string
  ollama_timeout_secs?: number
}

/** The accepted range for a numeric setting, and its no-value fallback. */
export interface Bounds {
  min: number
  max: number
  fallback: number
}

/**
 * Every numeric setting's accepted range, read from the same declaration that
 * enforces it — so a control can't offer a value the writer then clamps away.
 */
export interface ConfigBounds {
  fetch_interval_ms: Bounds
  scan_depth: Bounds
  tab_size: Bounds
  ai_timeout_secs: Bounds
}

/** One repo row as the picker knows it: its path plus every label to match. */
export interface RepoRow {
  path: string
  names: string[]
}

/** Where a clone will land, and what it will be called. */
export interface CloneTarget {
  normalized_url: string
  repo_name: string
  target_path: string
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
  /**
   * Apply a field-wise patch and get the normalized result back. The only
   * writer: a surface patches what it owns, so it can no longer revert what
   * the other client changed while the window was open. Feed the returned
   * config straight back to the form and an out-of-range entry corrects
   * itself.
   */
  patchConfig: (patch: ConfigPatch) => invoke<Config>('patch_config', { patch }),
  /** The range every numeric setting is clamped to — a control's min/max. */
  configBounds: () => invoke<ConfigBounds>('config_bounds'),
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
  /** Letter + label for every status, fetched once — not per row per repaint. */
  fileStatusStyles: () => invoke<FileStatusStyle[]>('file_status_styles'),
  getHeadSha: (repoPath: string) => invoke<string>('get_head_sha', { repoPath }),
  getSelectedDiff: (repoPath: string, files: FileEntry[]) =>
    invoke<string>('get_selected_diff', { repoPath, files }),
  getLog: (repoPath: string, maxCount: number, skip: number) =>
    invoke<CommitInfo[]>('get_log', { repoPath, opts: { max_count: maxCount, skip } }),
  /** A commit's files and its line totals, from one `git log`. */
  getCommitDetail: (repoPath: string, sha: string) =>
    invoke<CommitDetail>('get_commit_detail', { repoPath, sha }),
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
  /** What a discard would do per path — the confirmation dialog's copy,
   *  decided by the same code that performs it. */
  classifyDiscard: (repoPath: string, files: FileEntry[]) =>
    invoke<DiscardPlan>('classify_discard', { repoPath, files }),
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
  /** `background` picks the budget: an automatic fetch nobody waits on fails
   *  fast (12 s) so an unreachable remote can't hold the single network slot;
   *  a user-initiated one keeps the generous budget a real transfer needs. */
  fetch: (repoPath: string, remote: string, background: boolean) =>
    invoke<void>('fetch', { repoPath, remote, background }),
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
  /** The first remote's name, or `null` when the repo has none — never an
   *  invented "origin", which made every no-remote guard unfireable. */
  getRemote: (repoPath: string) => invoke<string | null>('get_remote', { repoPath }),
  getRepoIdentifier: (repoPath: string) =>
    invoke<RepoIdentifier | null>('get_repo_identifier', { repoPath }),
  mergeBranch: (repoPath: string, branch: string) =>
    invoke<MergeResult>('merge_branch', { repoPath, branch }),
  mergeSquash: (repoPath: string, branch: string) =>
    invoke<MergeResult>('merge_squash', { repoPath, branch }),
  commitSquashMerge: (repoPath: string) => invoke<void>('commit_squash_merge', { repoPath }),
  mergeAbort: (repoPath: string) => invoke<void>('merge_abort', { repoPath }),
  countCommitsToMerge: (repoPath: string, targetBranch: string) =>
    invoke<number>('count_commits_to_merge', { repoPath, targetBranch }),
  /**
   * The folders discovery would actually walk for this config — the
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

/** What this client asks core to build: everything a WebView renders from. */
export const WEBVIEW_DIFF_OPTIONS: DiffOptions = {
  html: true,
  side_by_side: true,
  show_anyway: false,
}

export const diffApi = {
  /**
   * Read and parse one working-tree file's diff in a single call. Rejects
   * rather than returning empty when the read fails, so the pane can tell a
   * failure from a file with nothing to show.
   */
  getParsedDiff: (
    repoPath: string,
    file: FileEntry,
    hideWhitespace: boolean,
    options: DiffOptions = WEBVIEW_DIFF_OPTIONS
  ) => invoke<ParsedDiff>('get_parsed_diff', { repoPath, file, hideWhitespace, options }),
  /** The same, for one file within a commit. Empty `filePath` = whole commit. */
  getParsedCommitDiff: (
    repoPath: string,
    sha: string,
    filePath: string,
    options: DiffOptions = WEBVIEW_DIFF_OPTIONS
  ) => invoke<ParsedDiff>('get_parsed_commit_diff', { repoPath, sha, filePath, options }),
  /** Plain text of a flat line range, rebuilt from the model — immune to
   *  gutters, `+`/`−` prefixes and side-by-side filler cells. */
  copyDiffText: (fileDiff: FileDiff, start: number, end: number) =>
    invoke<string>('copy_diff_text', { fileDiff, start, end }),
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
  /** Streams `git-progress` events like a URL clone: `gh repo clone` forwards
   *  `--progress` to `git clone`, so both routes report real numbers. */
  clone: (nameWithOwner: string, targetPath: string) =>
    invoke<string>('gh_clone', { nameWithOwner, targetPath }),
  publishRepo: (repoPath: string, name: string, description: string, isPrivate: boolean) =>
    invoke<void>('gh_publish_repo', { repoPath, name, description, isPrivate }),
}

export interface AiProviderConfig {
  provider: string
  model?: string
  base_url?: string
  timeout_secs: number
}

export const aiApi = {
  /**
   * The AI settings resolved for the selected provider. The config→provider
   * mapping lives in core, so the model and server URL always belong to the
   * provider actually about to run — splicing a picker value over a
   * separately-loaded config is how the two clients drifted.
   */
  loadAiConfig: () => invoke<AiProviderConfig>('load_ai_config'),
  generateCommitMessage: (diff: string, provider: string, config: AiProviderConfig) =>
    invoke<CommitMessage>('generate_commit_message', { diff, provider, config }),
  checkProviderAvailable: (provider: string, config: AiProviderConfig) =>
    invoke<boolean>('check_provider_available', { provider, config }),
}

export const reposApi = {
  /**
   * Every repo the picker should list: discovery over the scan folders unioned
   * with the persisted MRU, minus entries that no longer exist. Discovery
   * alone forgot clones, CLI opens and Open-Other rows on every restart, even
   * though the MRU that remembers them was already on disk.
   */
  knownRepos: (scanPaths: string[], maxDepth: number) =>
    invoke<string[]>('known_repos', { scanPaths, maxDepth }),
  /**
   * Narrow and rank rows against a typed query, strongest match first; ties
   * keep the input order, so a picker's own arrangement survives filtering.
   * One crossing per keystroke rather than one per row.
   */
  filterRepos: (query: string, rows: RepoRow[], scanFolders: string[]) =>
    invoke<string[]>('filter_repos', { query, rows, scanFolders }),
  /** What cloning `rawUrl` under `parent` would produce; `null` when there is
   *  nothing cloneable — which is also the Clone button's enable condition. */
  deriveCloneTarget: (rawUrl: string, parent: string) =>
    invoke<CloneTarget | null>('derive_clone_target', { rawUrl, parent }),
  /** Where a clone of `repoName` lands under `parent` — the GitHub tab's half. */
  cloneTargetPath: (parent: string, repoName: string) =>
    invoke<string | null>('clone_target_path', { parent, repoName }),
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

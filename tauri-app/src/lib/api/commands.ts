import { invoke } from '@tauri-apps/api/core'

export interface FileEntry {
  path: string
  orig_path?: string
  status: 'New' | 'Modified' | 'Deleted' | 'Renamed' | 'Conflicted'
  xy: string
  display_name: string
  display_dir: string
}

export interface RepoStatus {
  branch: string
  upstream: string
  has_upstream: boolean
  ahead: number
  behind: number
  files: FileEntry[]
  /** SHAs of commits reachable from HEAD but not on the remote. Empty when in sync or no upstream. */
  unpushed_shas: string[]
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
  formatCommitMessage: (summary: string, description: string, coAuthors: string[] = []) =>
    invoke<string>('format_commit_message', { summary, description, coAuthors }),
  fetch: (repoPath: string, remote: string) => invoke<void>('fetch', { repoPath, remote }),
  pull: (repoPath: string, remote: string) => invoke<void>('pull', { repoPath, remote }),
  push: (repoPath: string, remote: string, branch: string, setUpstream: boolean, forceWithLease: boolean) =>
    invoke<void>('push', { repoPath, remote, branch, setUpstream, forceWithLease }),
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
  getRepoName: (path: string) => invoke<string>('get_repo_name', { path }),
  cloneRepo: (url: string, targetPath: string) =>
    invoke<string>('clone_repo', { url, targetPath }),
  getLastCommitTimestamp: (repoPath: string) =>
    invoke<number>('get_last_commit_timestamp', { repoPath }),
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

export const highlightApi = {
  highlightDiff: (fileDiff: FileDiff) =>
    invoke<TokenLine[]>('highlight_diff', { fileDiff }),
}

export const ghApi = {
  checkAuth: () => invoke<boolean>('check_auth'),
  repoList: (limit: number) => invoke<GhRepo[]>('gh_repo_list', { limit }),
  clone: (nameWithOwner: string, targetPath: string) =>
    invoke<string>('gh_clone', { nameWithOwner, targetPath }),
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

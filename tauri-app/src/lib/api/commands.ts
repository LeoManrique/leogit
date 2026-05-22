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

export interface PullRequest {
  number: number
  title: string
  state: string
  author: string
  created_at: string
  updated_at: string
  url: string
  body: string
  is_draft: boolean
  base_ref_name: string
  head_ref_name: string
  review_decision?: string
}

export interface PRCheck {
  name: string
  state: string
  bucket: string
  link?: string
  workflow?: string
}

export interface DiffLine {
  text: string
  content: string
  line_type: 'Context' | 'Add' | 'Delete' | 'Hunk' | 'NoNewline'
  old_line_no?: number
  new_line_no?: number
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
  tab_size: number
  claude_timeout_secs: number
  ollama_server_url: string
}

export interface ReposState {
  last_opened_repo?: string
}

export const configApi = {
  loadConfig: () => invoke<Config>('load_config'),
  saveConfig: (cfg: Config) => invoke<void>('save_config', { cfg }),
  loadState: () => invoke<ReposState>('load_state'),
  saveState: (state: ReposState) => invoke<void>('save_state', { state }),
}

export const gitApi = {
  getStatus: (repoPath: string) => invoke<RepoStatus>('get_status', { repoPath }),
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
  commit: (repoPath: string, message: string) => invoke<void>('commit', { repoPath, message }),
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
}

export const diffApi = {
  parseDiff: (raw: string) => invoke<FileDiff | null>('parse_diff', { raw }),
  generatePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_patch', { repoPath, fileDiff, selection }),
  generateInversePatch: (repoPath: string, fileDiff: FileDiff, selection: DiffSelection) =>
    invoke<void>('generate_inverse_patch', { repoPath, fileDiff, selection }),
}

export const ghApi = {
  checkAuth: () => invoke<boolean>('check_auth'),
  listPRs: (repoPath: string, state: string) =>
    invoke<PullRequest[]>('list_prs', { repoPath, state }),
  getPRChecks: (repoPath: string, number: number) =>
    invoke<PRCheck[]>('get_pr_checks', { repoPath, number }),
  createPR: (repoPath: string, title: string, body: string, base: string, draft: boolean) =>
    invoke<string>('create_pr', { repoPath, title, body, base, draft }),
  createPRFill: (repoPath: string, base: string, draft: boolean) =>
    invoke<string>('create_pr_fill', { repoPath, base, draft }),
  checkoutPR: (repoPath: string, number: number) =>
    invoke<void>('checkout_pr', { repoPath, number }),
  getCurrentBranchPR: (repoPath: string, branch: string) =>
    invoke<PullRequest | null>('get_current_branch_pr', { repoPath, branch }),
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

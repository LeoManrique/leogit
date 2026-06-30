import { writable, derived } from 'svelte/store'
import type {
  FileEntry,
  CommitInfo,
  CommitStats,
  BranchInfo,
  DiffSelection,
  FileDiff,
} from '$lib/api/commands'

export type ActiveTab = 'changes' | 'history'

export interface RepoStatus {
  branch: string
  upstream: string
  hasUpstream: boolean
  ahead: number
  behind: number
  files: FileEntry[]
  isMerging: boolean
  /** Whether the repo has any configured remote. False → offer "Publish to GitHub". */
  hasRemote: boolean
  /** SHAs of commits the user still needs to push, used to mark History rows. */
  unpushedShas: Set<string>
  /** True when HEAD is detached (on a commit, not a branch), e.g. after "Checkout commit". */
  detached: boolean
  /** Full SHA of HEAD; empty only on an unborn branch. Labels the detached-HEAD state. */
  headSha: string
}

export interface RepoState {
  status: RepoStatus
  log: {
    commits: CommitInfo[]
    hasMore: boolean
    loaded: boolean
    /**
     * Absolute commit index (0 = HEAD) of `commits[0]`. The on-screen array
     * is capped at MAX_COMMITS and slides forward / backward as the user
     * scrolls, so the absolute position is tracked separately from the array.
     */
    windowStartOffset: number
  }
  branches: BranchInfo[]
  selectedFiles: Set<string>
  userDeselected: Set<string>
  diffSelection: Map<string, DiffSelection>
  activeFile: FileEntry | null
  activeFileDiff: FileDiff | null
  isDiffLoading: boolean
  /**
   * Flips true once a diff fetch has been pending for ≥150 ms. The viewer keeps
   * showing the previous diff while `isDiffLoading && !isDiffLoadingSlow`, so
   * sub-150 ms fetches (the common case) swap in place with no "Loading…"
   * flash. Mirrors GH Desktop's SeamlessDiffSwitcher.SlowDiffLoadingThreshold.
   */
  isDiffLoadingSlow: boolean
  activeTab: ActiveTab
  activeCommit: CommitInfo | null
  activeCommitFiles: FileEntry[]
  /** Aggregate +adds/-dels for the active commit; null until fetched. */
  activeCommitStats: CommitStats | null
  activeCommitFile: FileEntry | null
  activeCommitFileDiff: FileDiff | null
  isCommitDiffLoading: boolean
  isCommitDiffLoadingSlow: boolean
  isLoading: boolean
  commitToAmend: CommitInfo | null
  /**
   * One-shot seed for the commit composer after an Undo Commit. The composer
   * consumes it (prefills summary / description / co-authors) and clears it.
   * Separate from `commitToAmend` because undo is NOT amend mode — it just
   * pre-populates the composer for a fresh commit.
   */
  restoreMessage: { summary: string; description: string; coAuthors: string[] } | null
  error?: string
}

const defaultStatus: RepoStatus = {
  branch: '',
  upstream: '',
  hasUpstream: false,
  ahead: 0,
  behind: 0,
  files: [],
  isMerging: false,
  hasRemote: false,
  unpushedShas: new Set(),
  detached: false,
  headSha: '',
}

const defaultState: RepoState = {
  status: defaultStatus,
  log: {
    commits: [],
    hasMore: true,
    loaded: false,
    windowStartOffset: 0,
  },
  branches: [],
  selectedFiles: new Set(),
  userDeselected: new Set(),
  diffSelection: new Map(),
  activeFile: null,
  activeFileDiff: null,
  isDiffLoading: false,
  isDiffLoadingSlow: false,
  activeTab: 'changes',
  activeCommit: null,
  activeCommitFiles: [],
  activeCommitStats: null,
  activeCommitFile: null,
  activeCommitFileDiff: null,
  isCommitDiffLoading: false,
  isCommitDiffLoadingSlow: false,
  isLoading: false,
  commitToAmend: null,
  restoreMessage: null,
}

export const repoState = writable<RepoState>(defaultState)

export function resetRepoState() {
  repoState.set({
    ...defaultState,
    selectedFiles: new Set(),
    userDeselected: new Set(),
    diffSelection: new Map(),
  })
}

export const canCommit = derived(repoState, ($state) => {
  return $state.selectedFiles.size > 0 && !$state.isLoading
})

export const hasMergeConflicts = derived(repoState, ($state) => {
  return $state.status.files.some((f) => f.status === 'Conflicted')
})

export const currentBranch = derived(repoState, ($state) => {
  return $state.status.branch
})

export const remoteBranches = derived(repoState, ($state) => {
  return $state.branches.filter((b) => b.is_remote)
})

export const localBranches = derived(repoState, ($state) => {
  return $state.branches.filter((b) => !b.is_remote)
})

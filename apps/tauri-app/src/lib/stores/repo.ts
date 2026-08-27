import { writable, derived } from 'svelte/store'
import type {
  FileEntry,
  CommitInfo,
  CommitStats,
  BranchInfo,
  DiffSelection,
  ParsedDiff,
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

/**
 * Whether `status` has ever been filled for the open repository, as opposed to
 * holding the defaults a repo switch resets it to. Anything that *skips* work
 * on a status field has to know the difference: `hasRemote` defaults to false,
 * and a gate reading that between the switch and the first load would decide
 * "no remote" about a repo it has not looked at yet.
 */

export interface RepoState {
  status: RepoStatus
  statusLoaded: boolean
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
    /**
     * Bumped whenever the window is *replaced* by a fresh page-1 load (HEAD
     * moved, a different repo, the first load) rather than slid by one page.
     * `CommitList` compensates `scrollTop` for a slide; a replacement is not
     * a slide, so it has to be told apart — compensating for one scrolls the
     * user to the bottom of the new page and immediately pages again.
     */
    resetSeq: number
  }
  branches: BranchInfo[]
  selectedFiles: Set<string>
  userDeselected: Set<string>
  diffSelection: Map<string, DiffSelection>
  activeFile: FileEntry | null
  activeFileDiff: ParsedDiff | null
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
  activeCommitFileDiff: ParsedDiff | null
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
  /**
   * Set once the silent status poll has failed several ticks in a row — the
   * repository is genuinely unreadable (deleted, unmounted, permissions), not
   * momentarily locked mid-write. Owned exclusively by the poll: only its own
   * recovery clears it, so an explicit action's failure (which goes to
   * `error`, and to the modal) is never swept away by a background tick.
   */
  pollError?: string
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
  statusLoaded: false,
  log: {
    commits: [],
    hasMore: true,
    loaded: false,
    windowStartOffset: 0,
    resetSeq: 0,
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
  repoState.update((s) => ({
    ...defaultState,
    selectedFiles: new Set(),
    userDeselected: new Set(),
    diffSelection: new Map(),
    // A repo switch replaces the commit window like any other replacement, so
    // the counter keeps going up rather than restarting at 0 — `CommitList`
    // reads *a change* in it, and 0-after-0 would read as no change and leave
    // it compensating a slide that never happened.
    log: { ...defaultState.log, resetSeq: s.log.resetSeq + 1 },
  }))
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

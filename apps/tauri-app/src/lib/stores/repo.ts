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
  /**
   * The commit log, as an **append-only list rooted at HEAD**: `commits[0]` is
   * the repository's HEAD, always, and paging only ever adds older rows to the
   * end. Nothing is ever dropped from the front, which is what makes the
   * rewriting actions safe to reason about — see `CommitList.headSha`.
   */
  log: {
    commits: CommitInfo[]
    hasMore: boolean
    loaded: boolean
    /**
     * A page fetch is in flight. Deliberately its own flag rather than a
     * repo-wide one: paging History used to disable the Commit button on the
     * other tab, which has nothing to do with reading older commits.
     */
    isPaging: boolean
    /**
     * Bumped whenever the list is re-read from HEAD (HEAD moved, a different
     * repo, the first load) rather than extended by a page. `CommitList`
     * answers it by scrolling to row 0 — the new HEAD is what the user should
     * be looking at, and their old offset means nothing against a list whose
     * top just changed.
     */
    resetSeq: number
  }
  branches: BranchInfo[]
  selectedFiles: Set<string>
  userDeselected: Set<string>
  diffSelection: Map<string, DiffSelection>
  activeFile: FileEntry | null
  activeFileDiff: ParsedDiff | null
  /**
   * Why the open file has no diff on screen. A diff that failed to load is not
   * a diff that is empty, and it is not an operation the user is waiting on
   * either — it belongs in the pane that was going to show it, not in the
   * window-taking modal (FRONTEND §6.3). Set only alongside a cleared payload:
   * a stale diff must never stand behind an error describing a newer one.
   */
  activeFileDiffError?: string
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
  /** The commit pane's half of {@link RepoState.activeFileDiffError}. */
  activeCommitFileDiffError?: string
  isCommitDiffLoading: boolean
  isCommitDiffLoadingSlow: boolean
  commitToAmend: CommitInfo | null
  /**
   * One-shot seed for the commit composer after an Undo Commit. The composer
   * consumes it (prefills summary / description / co-authors) and clears it.
   * Separate from `commitToAmend` because undo is NOT amend mode — it just
   * pre-populates the composer for a fresh commit.
   */
  restoreMessage: { summary: string; description: string; coAuthors: string[] } | null
  /**
   * Failure of an operation the user asked for and is waiting on — a transfer,
   * a branch change, an explicit refresh. Takes the window, because the thing
   * the user was doing did not happen. See {@link reportActionError}.
   */
  error?: string
  /**
   * The same operation, bound for a second attempt. Set only where retrying is
   * well defined; without it the modal offers Dismiss alone.
   */
  errorRetry?: () => void
  /**
   * Failure of something that was never the user's task — the file manager
   * wouldn't open, the browser wouldn't launch. Reported in the non-blocking
   * banner and dismissed by hand: unlike `pollError` there is no later success
   * that disproves it, so nothing can retire it on the user's behalf.
   * See {@link reportNotice}.
   */
  notice?: string
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
    isPaging: false,
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
    // A repo switch re-reads the list from HEAD like any other reset, so the
    // counter keeps going up rather than restarting at 0 — `CommitList` reads
    // *a change* in it, and 0-after-0 would read as no change and leave the
    // new repository's history scrolled to the old one's offset.
    log: { ...defaultState.log, resetSeq: s.log.resetSeq + 1 },
  }))
}

/**
 * Report a failure the user is waiting on: it takes the window, and offers a
 * second attempt when `retry` is given.
 *
 * The classification is native's: an operation the user asked for gets a modal,
 * an informational hand-off gets the banner ({@link reportNotice}). Both live
 * here so each call site makes that choice by picking a function, rather than
 * by copying a `repoState.update` shape — which is how every failure in this
 * client, down to "couldn't reveal the file in Finder", ended up seizing the
 * window.
 */
export function reportActionError(message: unknown, retry?: () => void) {
  repoState.update((s) => ({ ...s, error: String(message), errorRetry: retry }))
}

/** Clear the action-failure modal along with whatever retry it carried. */
export function dismissActionError() {
  repoState.update((s) => ({ ...s, error: undefined, errorRetry: undefined }))
}

/**
 * Report a failure that isn't the user's task — an OS hand-off that didn't
 * take. States itself in the banner and stays out of the way; the last good
 * view of the repository is still on screen behind it.
 */
export function reportNotice(message: unknown) {
  repoState.update((s) => ({ ...s, notice: String(message) }))
}

/** Dismiss the informational banner. Nothing else ever clears it. */
export function dismissNotice() {
  repoState.update((s) => ({ ...s, notice: undefined }))
}

export const canCommit = derived(repoState, ($state) => {
  return $state.selectedFiles.size > 0
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

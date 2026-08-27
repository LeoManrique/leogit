<script lang="ts">
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { listen } from '@tauri-apps/api/event'
  import { repoState, resetRepoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { config, refreshConfig } from '$lib/stores/config'
  import { hydrateReposState, patchReposState, recordRecentRepo } from '$lib/stores/reposState'
  import { setRepoSync } from '$lib/stores/repoSync'
  import { activeNetworkOp } from '$lib/stores/networkOps'
  import { repoSyncScheduler } from '$lib/services/repoSyncScheduler'
  import { rediscoverRepos } from '$lib/services/repoDiscovery'
  import { resolveCloneDefaultDir, rememberCloneDir } from '$lib/services/cloneFlow'
  import {
    shouldAttemptBackground,
    recordResult,
    initConnectivity,
  } from '$lib/services/connectivity'
  import {
    gitApi,
    diffApi,
    WEBVIEW_DIFF_OPTIONS,
    type FileEntry,
    type CommitInfo,
    type DiffSizeGuard,
    type DiscardPlan,
    type EmptyDiffReason,
    type LaunchTarget,
    type ParsedDiff,
  } from '$lib/api/commands'
  import * as fileActions from '$lib/services/fileActions'
  import type { FileContextActions } from '$lib/services/fileActions'
  import { isFromTerminal } from '$lib/utils/keyboard'

  import Header from '$lib/components/Header.svelte'
  import TabBar from '$lib/components/TabBar.svelte'
  import FileList from '$lib/components/FileList.svelte'
  import DiscardConfirm from '$lib/components/DiscardConfirm.svelte'
  import CheckoutCommitConfirm from '$lib/components/CheckoutCommitConfirm.svelte'
  import CommitMessage from '$lib/components/CommitMessage.svelte'
  import CommitList from '$lib/components/CommitList.svelte'
  import DiffViewer from '$lib/components/DiffViewer.svelte'
  import Terminal from '$lib/components/Terminal.svelte'
  import CommitDetail from '$lib/views/CommitDetail.svelte'
  import BranchDropdown from '$lib/views/BranchDropdown.svelte'
  import RepoDropdown from '$lib/views/RepoDropdown.svelte'
  import CloneOverlay from '$lib/views/CloneOverlay.svelte'
  import MergeOverlay from '$lib/views/MergeOverlay.svelte'
  import SettingsOverlay from '$lib/views/SettingsOverlay.svelte'
  import HelpOverlay from '$lib/views/HelpOverlay.svelte'
  import ErrorModal from '$lib/components/ErrorModal.svelte'

  let terminalExpanded = $state(false)
  let terminalSessionId = $state(0) // 0 = no active PTY; >0 = key for the mounted Terminal
  // Shell the running session actually launched, reported by the backend. May
  // differ from the configured preference when that shell isn't installed, so
  // the header shows what's really running rather than what was asked for.
  let activeShellLabel = $state('')
  let showRepos = $state(false)
  let showClone = $state(false)
  // Instance handle so the global Escape handler can ask whether a clone is
  // in flight before dismissing the dialog.
  let cloneOverlay = $state<{ isBusy: () => boolean } | null>(null)
  // Destination pre-filled into the Clone dialog: last-used folder, else the
  // first configured scan path, else ~/Dev (the backend expands the leading ~).
  let cloneDefaultDir = $state('~/Dev')
  let showBranches = $state(false)
  let showSettings = $state(false)
  let showHelp = $state(false)
  let showMerge = $state(false)
  let mergeTarget = $state<string>('')

  let statusInterval: ReturnType<typeof setInterval> | null = null
  let fetchInterval: ReturnType<typeof setInterval> | null = null
  let userTyping = $state(false)
  let lastHeadSha: string | null = null

  // Defer the "Loading diff…" placeholder so sub-150 ms fetches swap the
  // diff in place with no flash. If the fetch outlives the threshold, the
  // viewer falls back to the spinner. Mirrors GH Desktop's
  // SeamlessDiffSwitcher.SlowDiffLoadingThreshold.
  const SLOW_DIFF_THRESHOLD_MS = 150
  let diffLoadingTimer: ReturnType<typeof setTimeout> | null = null
  let commitDiffLoadingTimer: ReturnType<typeof setTimeout> | null = null

  const PAGE_SIZE = 50
  // Hard cap on the in-memory commit log. The CommitList already virtualizes
  // the DOM, but the underlying array was growing without bound: scrolling
  // 5K commits kept all 5K CommitInfo objects in memory and made every
  // refresh re-fetch them. The window slides forward when the user scrolls
  // past the bottom and back when they scroll past the top, so they never
  // lose access to history — the working set just stays bounded.
  const MAX_COMMITS = 500

  const SIDEBAR_MIN = 280
  const SIDEBAR_MAX = 640
  const COMMIT_MIN = 180
  const COMMIT_MAX = 600
  const COMMIT_FILES_MIN = 180
  const COMMIT_FILES_MAX = 600

  function loadStoredNumber(key: string, fallback: number, min: number, max: number): number {
    if (typeof window === 'undefined') return fallback
    const raw = window.localStorage.getItem(key)
    const n = raw ? parseInt(raw, 10) : NaN
    return Number.isFinite(n) && n >= min && n <= max ? n : fallback
  }

  let sidebarWidth = $state(loadStoredNumber('leogit:sidebarWidth', 320, SIDEBAR_MIN, SIDEBAR_MAX))
  let commitHeight = $state(loadStoredNumber('leogit:commitHeight', 220, COMMIT_MIN, COMMIT_MAX))
  let commitFilesWidth = $state(
    loadStoredNumber('leogit:commitFilesWidth', 280, COMMIT_FILES_MIN, COMMIT_FILES_MAX),
  )

  function startSidebarResize(e: MouseEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = sidebarWidth
    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - startX
      sidebarWidth = Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, startWidth + delta))
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:sidebarWidth', String(sidebarWidth))
    }
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  function startCommitFilesResize(e: MouseEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = commitFilesWidth
    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - startX
      commitFilesWidth = Math.max(
        COMMIT_FILES_MIN,
        Math.min(COMMIT_FILES_MAX, startWidth + delta),
      )
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:commitFilesWidth', String(commitFilesWidth))
    }
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  function startCommitResize(e: MouseEvent) {
    e.preventDefault()
    const startY = e.clientY
    const startHeight = commitHeight
    function onMove(ev: MouseEvent) {
      const delta = startY - ev.clientY
      commitHeight = Math.max(COMMIT_MIN, Math.min(COMMIT_MAX, startHeight + delta))
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:commitHeight', String(commitHeight))
    }
    document.body.style.cursor = 'row-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  // Keyboard resizing for the splitter handles (role="separator", tabindex=0).
  // Arrow keys nudge by RESIZE_STEP px; Home/End jump to the min/max. This is
  // what makes the handles accessible splitters rather than mouse-only drag
  // targets. `axis` selects which arrow keys apply: a 'horizontal' splitter
  // (stacked panes) responds to Up/Down, a 'vertical' splitter (side-by-side
  // panes) to Left/Right.
  const RESIZE_STEP = 16

  function splitterKey(
    e: KeyboardEvent,
    axis: 'horizontal' | 'vertical',
    current: number,
    min: number,
    max: number,
    apply: (value: number) => void,
  ) {
    const decKey = axis === 'horizontal' ? 'ArrowDown' : 'ArrowLeft'
    const incKey = axis === 'horizontal' ? 'ArrowUp' : 'ArrowRight'
    let next = current
    if (e.key === decKey) next = current - RESIZE_STEP
    else if (e.key === incKey) next = current + RESIZE_STEP
    else if (e.key === 'Home') next = min
    else if (e.key === 'End') next = max
    else return
    e.preventDefault()
    apply(Math.max(min, Math.min(max, next)))
  }

  function handleSidebarKey(e: KeyboardEvent) {
    splitterKey(e, 'vertical', sidebarWidth, SIDEBAR_MIN, SIDEBAR_MAX, (v) => {
      sidebarWidth = v
      window.localStorage.setItem('leogit:sidebarWidth', String(v))
    })
  }

  function handleCommitKey(e: KeyboardEvent) {
    splitterKey(e, 'horizontal', commitHeight, COMMIT_MIN, COMMIT_MAX, (v) => {
      commitHeight = v
      window.localStorage.setItem('leogit:commitHeight', String(v))
    })
  }

  function handleCommitFilesKey(e: KeyboardEvent) {
    splitterKey(e, 'vertical', commitFilesWidth, COMMIT_FILES_MIN, COMMIT_FILES_MAX, (v) => {
      commitFilesWidth = v
      window.localStorage.setItem('leogit:commitFilesWidth', String(v))
    })
  }

  /*
    Whether a parsed diff has something to put on screen. Core answers the
    "why not" question itself now — `empty_reason` and `size_guard` — so this
    is only the boolean the template branches on. Binary diffs are renderable:
    the viewer has its own state for them.
  */
  function hasRenderableDiff(diff: ParsedDiff | null): boolean {
    if (!diff) return false
    if (diff.empty_reason || diff.size_guard) return false
    const fileDiff = diff.file_diff
    return fileDiff.is_binary || fileDiff.hunks.some((h) => h.lines.length > 0)
  }

  /*
    The empty pane's heading and explanation, from core's reason rather than
    one caption covering three unrelated situations at once — which told the
    user the file was unchanged when the whitespace setting was simply hiding
    the change.
  */
  const EMPTY_DIFF_COPY: Record<EmptyDiffReason, { title: string; detail: string }> = {
    NoChanges: {
      title: 'No changes',
      detail: 'This file matches its committed state.',
    },
    WhitespaceOnly: {
      title: 'Whitespace only',
      detail: 'Every change here is whitespace, and Settings is set to hide those.',
    },
    NoTextualChanges: {
      title: 'No textual changes',
      detail:
        'The file changed without changing any lines — a mode change or rename, for example.',
    },
  }

  function emptyDiffCopy(diff: ParsedDiff | null): { title: string; detail: string } {
    return EMPTY_DIFF_COPY[diff?.empty_reason ?? 'NoTextualChanges']
  }

  /* What the size guard withheld, in the units the user thinks in. */
  function sizeGuardCopy(guard: DiffSizeGuard): string {
    return guard.reason === 'TotalBytes'
      ? `This diff is ${(guard.bytes / 1_048_576).toFixed(1)} MB — large enough to be slow to render.`
      : `This diff has a line of ${guard.longest_line} characters — long enough to be slow to render.`
  }

  // A submodule that is dirty inside but whose recorded commit hasn't moved
  // can't be staged from the parent repo (`git add` is a no-op), so it's never
  // eligible for commit selection. Every writer to `selectedFiles` skips these
  // so the user can't include one and hit "staging produced no changes".
  const isCommittable = (f: FileEntry) => !f.submodule_dirty

  // Consecutive *background* refresh failures. One is usually a transient index
  // lock mid-write and stays invisible; a streak means the repository is
  // genuinely unreadable (deleted, unmounted, permissions), which the user has
  // to be told about — otherwise the pane keeps rendering its last good
  // snapshot forever. Same threshold as the native client's
  // `quietFailureStreak`, and the same ownership: only the app's own timers
  // and resyncs move it. A refresh that follows a *user action* (commit,
  // discard, ignore, checkout) is silent for a different reason — the action
  // already reported its own outcome — and must not feed this, or three
  // index.lock races in a row would accuse a healthy repo of having vanished.
  let quietFailureStreak = 0
  const QUIET_FAILURE_THRESHOLD = 3

  async function refreshStatus(
    opts: { silent?: boolean; background?: boolean } = {},
  ): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const status = await gitApi.getStatus(repoPath)
      repoState.update((s) => {
        const presentPaths = new Set(status.files.map((f) => f.path))
        const nextSelected = new Set<string>()
        for (const f of status.files) {
          if (isCommittable(f) && !s.userDeselected.has(f.path)) nextSelected.add(f.path)
        }
        const nextDeselected = new Set<string>()
        for (const p of s.userDeselected) {
          if (presentPaths.has(p)) nextDeselected.add(p)
        }
        // If the file the user was viewing has just been committed (or
        // otherwise dropped out of the working tree), drop the diff too —
        // otherwise the pane keeps rendering a diff for a path that no
        // longer exists in the changeset.
        const activeFileGone =
          s.activeFile !== null && !presentPaths.has(s.activeFile.path)
        return {
          ...s,
          statusLoaded: true,
          status: {
            branch: status.branch,
            upstream: status.upstream,
            hasUpstream: status.has_upstream,
            ahead: status.ahead,
            behind: status.behind,
            files: status.files,
            isMerging: status.merging,
            hasRemote: status.has_remote,
            unpushedShas: new Set(status.unpushed_shas ?? []),
            detached: status.detached,
            headSha: status.head_sha,
          },
          selectedFiles: nextSelected,
          userDeselected: nextDeselected,
          activeFile: activeFileGone ? null : s.activeFile,
          activeFileDiff: activeFileGone ? null : s.activeFileDiff,
          isDiffLoading: activeFileGone ? false : s.isDiffLoading,
          error: opts.silent ? s.error : undefined,
          // Any successful read proves the repository is back, so the banner
          // goes whoever asked for the read. Only the background path *sets*
          // it, so no user-facing error can be lost with it.
          pollError: undefined,
        }
      })
      quietFailureStreak = 0
      // Keep the picker's badge for the active repo live off the same counts
      // the 2s poll already computed — no extra fetch needed for the open
      // repo. `dirty` comes straight from the file list the Changes tab
      // renders, so for the visible repo dot and tab agree by construction.
      setRepoSync(repoPath, {
        ahead: status.ahead,
        behind: status.behind,
        hasRemote: status.has_remote,
        dirty: status.files.length > 0,
      })
    } catch (error) {
      if (!opts.silent) {
        repoState.update((s) => ({ ...s, error: String(error) }))
        return
      }
      if (!opts.background) return // a user action's own follow-up: it reported already
      // Swallow the blip, surface the streak. The banner is non-blocking on
      // purpose — a background tick must never seize the window the way an
      // action's failure modal does.
      quietFailureStreak += 1
      if (quietFailureStreak >= QUIET_FAILURE_THRESHOLD) {
        repoState.update((s) => ({ ...s, pollError: String(error) }))
      }
    }
  }

  async function refreshBranches(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const branches = await gitApi.listBranches(repoPath)
      repoState.update((s) => ({ ...s, branches }))
    } catch {}
  }

  async function loadInitialLog(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const commits = await gitApi.getLog(repoPath, PAGE_SIZE, 0)
      repoState.update((s) => ({
        ...s,
        log: {
          commits,
          hasMore: commits.length === PAGE_SIZE,
          loaded: true,
          windowStartOffset: 0,
          resetSeq: s.log.resetSeq + 1,
        },
      }))
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  // Refresh the commit log without losing the user's scroll position. We
  // re-fetch the current window (`count` starting at `windowStartOffset`),
  // capped at MAX_COMMITS. If HEAD has moved while the user is scrolled
  // into the past (commits[0].sha changed for the same offset), reset to
  // the first page so the new HEAD is visible — a small visual jump is
  // accurate, while silently keeping the old window would mislead the user.
  // The reset bumps `resetSeq`: the list must scroll to the new HEAD, not
  // compensate for a slide that never happened.
  async function refreshLog(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    const count = Math.min(Math.max(current.log.commits.length, PAGE_SIZE), MAX_COMMITS)
    const skip = current.log.windowStartOffset
    try {
      const commits = await gitApi.getLog(repoPath, count, skip)
      const headChanged =
        skip > 0 &&
        commits.length > 0 &&
        current.log.commits.length > 0 &&
        commits[0]?.sha !== current.log.commits[0]?.sha
      if (headChanged) {
        const fresh = await gitApi.getLog(repoPath, PAGE_SIZE, 0)
        repoState.update((s) => ({
          ...s,
          log: {
            commits: fresh,
            hasMore: fresh.length === PAGE_SIZE,
            loaded: true,
            windowStartOffset: 0,
            resetSeq: s.log.resetSeq + 1,
          },
        }))
        return
      }
      repoState.update((s) => ({
        ...s,
        log: {
          ...s.log,
          commits,
          hasMore: commits.length === count,
          loaded: true,
          windowStartOffset: skip,
        },
      }))
    } catch {}
  }

  async function pollHeadSha(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const sha = await gitApi.getHeadSha(repoPath)
      if (lastHeadSha === null) {
        lastHeadSha = sha
        return
      }
      if (sha !== lastHeadSha) {
        lastHeadSha = sha
        await refreshLog()
      }
    } catch {}
  }

  async function loadMoreCommits(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    if (!current.log.hasMore || current.isLoading) return

    repoState.update((s) => ({ ...s, isLoading: true }))
    try {
      const skip = current.log.windowStartOffset + current.log.commits.length
      const fetched = await gitApi.getLog(repoPath, PAGE_SIZE, skip)
      repoState.update((s) => {
        const combined = [...s.log.commits, ...fetched]
        // Slide the window: drop the oldest entries past MAX_COMMITS and
        // advance windowStartOffset by the same count. CommitList watches
        // windowStartOffset and compensates its scrollTop so the user's
        // visible row stays pinned across the slide.
        if (combined.length <= MAX_COMMITS) {
          return {
            ...s,
            log: { ...s.log, commits: combined, hasMore: fetched.length === PAGE_SIZE, loaded: true },
            isLoading: false,
          }
        }
        const drop = combined.length - MAX_COMMITS
        return {
          ...s,
          log: {
            ...s.log,
            commits: combined.slice(drop),
            hasMore: fetched.length === PAGE_SIZE,
            loaded: true,
            windowStartOffset: s.log.windowStartOffset + drop,
          },
          isLoading: false,
        }
      })
    } catch (error) {
      repoState.update((s) => ({ ...s, isLoading: false, error: String(error) }))
    }
  }

  async function loadEarlierCommits(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    if (current.isLoading || current.log.windowStartOffset === 0) return

    repoState.update((s) => ({ ...s, isLoading: true }))
    try {
      const want = Math.min(PAGE_SIZE, current.log.windowStartOffset)
      const skip = current.log.windowStartOffset - want
      const fetched = await gitApi.getLog(repoPath, want, skip)
      repoState.update((s) => {
        if (fetched.length === 0) return { ...s, isLoading: false }
        const combined = [...fetched, ...s.log.commits]
        // Mirror of loadMoreCommits: drop from the end if we'd exceed the
        // cap. Slide-backward never increases hasMore (we know the tail
        // already had more if it did before).
        const overflow = Math.max(0, combined.length - MAX_COMMITS)
        return {
          ...s,
          log: {
            ...s.log,
            commits: overflow > 0 ? combined.slice(0, combined.length - overflow) : combined,
            hasMore: s.log.hasMore || overflow > 0,
            loaded: true,
            windowStartOffset: skip,
          },
          isLoading: false,
        }
      })
    } catch (error) {
      repoState.update((s) => ({ ...s, isLoading: false, error: String(error) }))
    }
  }

  async function loadDiffForFile(
    file: FileEntry | null,
    opts: { force?: boolean; showAnyway?: boolean } = {},
  ): Promise<void> {
    // Re-activating the same file that's already on screen (or already
    // being fetched) is a no-op — skip the refetch so arrow scrolls past
    // and back don't churn the pane. `force` overrides this so a focus-return
    // refresh re-fetches the open file even though its path is unchanged.
    const current = get(repoState)
    if (
      !opts.force &&
      file &&
      current.activeFile?.path === file.path &&
      (current.activeFileDiff !== null || current.isDiffLoading)
    ) {
      return
    }

    if (diffLoadingTimer) {
      clearTimeout(diffLoadingTimer)
      diffLoadingTimer = null
    }

    // Clearing to null on every switch caused the "Loading diff…" flash
    // even for sub-50 ms fetches. Now we keep the previous diff on screen
    // and only flip isDiffLoadingSlow=true after SLOW_DIFF_THRESHOLD_MS,
    // at which point the template falls back to the spinner.
    repoState.update((s) => ({
      ...s,
      activeFile: file,
      isDiffLoading: file !== null,
      isDiffLoadingSlow: false,
      // Drop the stale diff immediately when the user deselects; only keep
      // it on screen during an actual transition between two files.
      activeFileDiff: file === null ? null : s.activeFileDiff,
    }))
    if (!file) return

    const repoPath = $appState.repoPath
    if (!repoPath) return

    diffLoadingTimer = setTimeout(() => {
      diffLoadingTimer = null
      const s = get(repoState)
      // Only escalate if the fetch we started is still the active one.
      if (s.activeFile?.path === file.path && s.isDiffLoading) {
        repoState.update((st) => ({ ...st, isDiffLoadingSlow: true, activeFileDiff: null }))
      }
    }, SLOW_DIFF_THRESHOLD_MS)

    try {
      const cfg = $config
      // One call: core reads and parses, and — when hide-whitespace left
      // nothing to show — checks the unfiltered diff so the pane can say the
      // change is there and the setting is hiding it.
      const parsed = await diffApi.getParsedDiff(repoPath, file, cfg?.hide_whitespace ?? false, {
        ...WEBVIEW_DIFF_OPTIONS,
        show_anyway: opts.showAnyway ?? false,
      })
      // Drop the result if the user moved on to a different file mid-fetch.
      if (get(repoState).activeFile?.path !== file.path) return
      if (diffLoadingTimer) {
        clearTimeout(diffLoadingTimer)
        diffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        activeFileDiff: parsed,
        isDiffLoading: false,
        isDiffLoadingSlow: false,
      }))
    } catch (error) {
      if (get(repoState).activeFile?.path !== file.path) return
      if (diffLoadingTimer) {
        clearTimeout(diffLoadingTimer)
        diffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        isDiffLoading: false,
        isDiffLoadingSlow: false,
        error: String(error),
      }))
    }
  }

  // Re-fetch the diff for the file currently open in the changes pane. Used on
  // app re-focus, where the file may have been edited on disk while we were
  // away. No-op when nothing is selected.
  function reloadActiveDiff(): void {
    const active = get(repoState).activeFile
    if (active) loadDiffForFile(active, { force: true })
  }

  // The size guard's escape: re-ask for the same diff with the guard lifted.
  // Deliberately not sticky — it applies to this one request, so moving to
  // another file gets the guard back rather than inheriting a decision made
  // about a different file.
  function showActiveDiffAnyway(): void {
    const active = get(repoState).activeFile
    if (active) loadDiffForFile(active, { force: true, showAnyway: true })
  }

  function showActiveCommitDiffAnyway(): void {
    const active = get(repoState).activeCommitFile
    if (active) loadCommitFileDiff(active, { force: true, showAnyway: true })
  }

  async function loadCommitFiles(commit: CommitInfo | null): Promise<void> {
    repoState.update((s) => ({
      ...s,
      activeCommit: commit,
      activeCommitFiles: [],
      activeCommitStats: null,
      activeCommitFile: null,
      activeCommitFileDiff: null,
    }))
    if (!commit) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      // One call for both: the file list and the line totals come out of the
      // same `git log`, so they can't describe different commits and a stats
      // failure can't leave the header disagreeing with the list.
      const detail = await gitApi.getCommitDetail(repoPath, commit.sha)
      // Guard against a stale response if the user clicked another commit.
      if (get(repoState).activeCommit?.sha !== commit.sha) return
      repoState.update((s) => ({
        ...s,
        activeCommitFiles: detail.files,
        activeCommitStats: detail.stats,
      }))
      if (detail.files.length > 0) {
        loadCommitFileDiff(detail.files[0])
      }
    } catch {}
  }

  async function loadCommitFileDiff(
    file: FileEntry | null,
    opts: { force?: boolean; showAnyway?: boolean } = {},
  ): Promise<void> {
    const current = get(repoState)
    if (
      !opts.force &&
      file &&
      current.activeCommitFile?.path === file.path &&
      (current.activeCommitFileDiff !== null || current.isCommitDiffLoading)
    ) {
      return
    }

    if (commitDiffLoadingTimer) {
      clearTimeout(commitDiffLoadingTimer)
      commitDiffLoadingTimer = null
    }

    repoState.update((s) => ({
      ...s,
      activeCommitFile: file,
      isCommitDiffLoading: file !== null,
      isCommitDiffLoadingSlow: false,
      activeCommitFileDiff: file === null ? null : s.activeCommitFileDiff,
    }))
    if (!file) return

    const repoPath = $appState.repoPath
    const commit = get(repoState).activeCommit
    if (!repoPath || !commit) {
      repoState.update((s) => ({ ...s, isCommitDiffLoading: false, isCommitDiffLoadingSlow: false }))
      return
    }

    commitDiffLoadingTimer = setTimeout(() => {
      commitDiffLoadingTimer = null
      const s = get(repoState)
      if (s.activeCommitFile?.path === file.path && s.isCommitDiffLoading) {
        repoState.update((st) => ({
          ...st,
          isCommitDiffLoadingSlow: true,
          activeCommitFileDiff: null,
        }))
      }
    }, SLOW_DIFF_THRESHOLD_MS)

    try {
      const parsed = await diffApi.getParsedCommitDiff(repoPath, commit.sha, file.path, {
        ...WEBVIEW_DIFF_OPTIONS,
        show_anyway: opts.showAnyway ?? false,
      })
      if (get(repoState).activeCommitFile?.path !== file.path) return
      if (commitDiffLoadingTimer) {
        clearTimeout(commitDiffLoadingTimer)
        commitDiffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        activeCommitFileDiff: parsed,
        isCommitDiffLoading: false,
        isCommitDiffLoadingSlow: false,
      }))
    } catch (error) {
      if (get(repoState).activeCommitFile?.path !== file.path) return
      if (commitDiffLoadingTimer) {
        clearTimeout(commitDiffLoadingTimer)
        commitDiffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        isCommitDiffLoading: false,
        isCommitDiffLoadingSlow: false,
        error: String(error),
      }))
    }
  }

  // Best-effort fetch of the active repo's remote. Swallows offline/auth/no-remote
  // errors so callers can always follow up with a status refresh regardless.
  // This is automatic (timer / refocus / cold-open), so it's gated on
  // connectivity: skipped while offline or backing off, and its outcome feeds
  // the breaker so a recovered link re-enables background syncing app-wide.
  async function fetchActiveRemote(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    if (!shouldAttemptBackground()) return
    // A repo with no remote has nothing to fetch: without this gate every
    // tick ran a doomed `git fetch`, two of them opened the breaker, and the
    // open breaker then suppressed *all* background fetches app-wide (tier
    // badges, refocus sweeps, the update check) for as long as that repo
    // stayed open. The tier path already gates on the same flag
    // (`repoSync.syncRepo`).
    //
    // Only skip when we *know* there is no remote. `hasRemote` defaults to
    // false, and a repo switch resets it, so between the switch and the first
    // status load an unqualified read would decide "no remote" about a repo
    // nobody has looked at — silently dropping a legitimate refocus fetch.
    const current = get(repoState)
    if (current.statusLoaded && !current.status.hasRemote) return
    let remote: string | null
    try {
      remote = await gitApi.getRemote(repoPath)
    } catch {
      return // local remote lookup failed — not a connectivity signal
    }
    // Now a real answer rather than an invented "origin", so this is the
    // authoritative version of the gate above rather than dead code under it.
    if (!remote) return
    try {
      // Automatic, so it runs on the background budget: an unreachable remote
      // gives up in 12 s instead of holding the single network slot for ten
      // minutes with every other repo's refresh queued behind it.
      await gitApi.fetch(repoPath, remote, true)
      recordResult(true)
    } catch {
      recordResult(false)
    }
  }

  async function performAutoFetch(): Promise<void> {
    await fetchActiveRemote()
    await refreshStatus({ silent: true, background: true })
  }

  // In-flight guard for the 2 s poll: a cycle that outlives the interval (repo
  // under heavy load, e.g. a big push compressing objects) must not stack a
  // second cycle on top of it — each one spawns several git processes.
  let statusPollInFlight = false

  function startStatusPolling(): void {
    if (statusInterval) clearInterval(statusInterval)
    statusInterval = setInterval(async () => {
      if ($appState.phase !== 'main') return
      // Pause while a push/pull/publish runs: polling mid-transfer only adds
      // git processes that contend with it for the repo's disk and locks. The
      // op's own handler refreshes status when it completes.
      if ($activeNetworkOp || statusPollInFlight) return
      statusPollInFlight = true
      try {
        await refreshStatus({ silent: true, background: true })
        await pollHeadSha()
      } finally {
        statusPollInFlight = false
      }
    }, 2000)
  }

  function startAutoFetch(intervalMs: number): void {
    if (fetchInterval) clearInterval(fetchInterval)
    if (intervalMs <= 0) return
    fetchInterval = setInterval(() => {
      if ($appState.phase !== 'main' || userTyping || $activeNetworkOp) return
      performAutoFetch()
    }, intervalMs)
  }

  // Re-sync when the app becomes active again — window focus, or the window
  // turning visible. Status and HEAD may have moved, and the file open in the
  // changes pane may have been edited on disk while we were away, so its diff
  // is re-fetched too. Guarded against overlapping runs since `focus` and
  // `visibilitychange` can both fire on a single window activation.
  let resyncing = false
  async function resyncOnActive(): Promise<void> {
    // Skipped during a network op for the same reason the poll pauses; the
    // op's completion refresh covers whatever this resync would have found.
    if (resyncing || $activeNetworkOp || $appState.phase !== 'main') return
    resyncing = true
    try {
      // The config file is shared with the native client and editable outside
      // this process, so re-read it before anything that consumes it. Without
      // this a save made elsewhere — theme, diff settings, provider — never
      // reached a running window at all: the staleness window was the lifetime
      // of the app. Re-read first so the refreshes below already see it.
      await refreshConfig()
      // Coming back to the app: fetch the active repo so a remote that moved
      // while we were away surfaces on the Pull button, then refresh local state.
      await fetchActiveRemote()
      await refreshStatus({ silent: true, background: true })
      pollHeadSha()
      reloadActiveDiff()
      // Also refresh the top recents tier so their picker badges aren't stale.
      repoSyncScheduler.refocusSync()
    } finally {
      resyncing = false
    }
  }

  function handleVisibilityChange(): void {
    if (!document.hidden) resyncOnActive()
  }

  function handleWindowFocus(): void {
    resyncOnActive()
  }

  function handleFocusEvent(e: FocusEvent): void {
    const t = e.target as HTMLElement | null
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) {
      userTyping = e.type === 'focusin'
    }
  }

  function handleFileActivate(file: FileEntry) {
    loadDiffForFile(file)
  }

  async function handleCommitted(): Promise<void> {
    // Defensive: clear amend mode if the composer somehow didn't.
    repoState.update((s) => ({ ...s, commitToAmend: null }))
    await Promise.all([refreshStatus({ silent: true }), refreshLog()])
    const repoPath = $appState.repoPath
    if (repoPath) {
      try {
        lastHeadSha = await gitApi.getHeadSha(repoPath)
      } catch {}
    }
  }

  function handleStartAmending(commit: CommitInfo): void {
    repoState.update((s) => ({ ...s, commitToAmend: commit, activeTab: 'changes' }))
  }

  function handleStopAmending(): void {
    repoState.update((s) => ({ ...s, commitToAmend: null }))
  }

  // ---- Checkout commit (detached HEAD) -------------------------------------
  // Commit pending a checkout confirmation; null when the dialog is closed.
  let checkoutTarget = $state<CommitInfo | null>(null)
  let isCheckingOut = $state(false)

  function handleCheckoutCommit(commit: CommitInfo): void {
    checkoutTarget = commit
  }

  async function confirmCheckout(): Promise<void> {
    const repoPath = $appState.repoPath
    const commit = checkoutTarget
    if (!repoPath || !commit) return
    isCheckingOut = true
    try {
      await gitApi.checkoutCommit(repoPath, commit.sha)
      // The checked-out commit is now HEAD. Seed lastHeadSha so the poll doesn't
      // redundantly reload the log, then refresh status (detached state) and the
      // log (now rooted at this commit), mirroring handleCommitted.
      lastHeadSha = commit.sha
      // Branches too: HEAD is now detached, so the picker's current-branch
      // marker and every branch's ahead/behind are stale.
      await Promise.all([
        refreshStatus({ silent: true }),
        refreshLog(),
        refreshBranches(),
      ])
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      // Always close the dialog so any error surfaces in the ErrorModal alone.
      checkoutTarget = null
      isCheckingOut = false
    }
  }

  function cancelCheckout(): void {
    if (!isCheckingOut) checkoutTarget = null
  }

  async function handleUndoCommit(commit: CommitInfo): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.undoLastCommit(repoPath)
      // Set the seed BEFORE refresh so the composer prefills as soon as the
      // tab switches over. The backend pre-parses the co-authors / stripped
      // body off the commit's trailers. Also defensively clear amend mode in
      // case the undone commit happened to be the one the user was amending.
      repoState.update((s) => ({
        ...s,
        commitToAmend: null,
        restoreMessage: {
          summary: commit.summary,
          description: commit.body_without_coauthors,
          coAuthors: commit.co_authors,
        },
        activeTab: 'changes',
      }))
      await handleCommitted()
      // The branch just lost a commit — its ahead count and the row's
      // metadata in the picker moved with it.
      await refreshBranches()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  // Master select-all from the FileList header. `selectAll === true` mirrors
  // the user explicitly opting every file in (clearing userDeselected); false
  // mirrors opting every file out (so refreshStatus won't re-add them next
  // poll tick).
  function handleToggleAll(selectAll: boolean) {
    repoState.update((s) => {
      if (selectAll) {
        return {
          ...s,
          selectedFiles: new Set(s.status.files.filter(isCommittable).map((f) => f.path)),
          userDeselected: new Set(),
        }
      }
      return {
        ...s,
        selectedFiles: new Set(),
        userDeselected: new Set(s.status.files.map((f) => f.path)),
      }
    })
  }

  // Range toggle from FileList — shift+click on a checkbox or Space on a
  // multi-row selection. Same opt-out tracking as handleFileToggle so the 2s
  // refreshStatus poll doesn't re-include paths the user just deselected.
  function handleBulkToggle(paths: string[], include: boolean) {
    repoState.update((s) => {
      const nextSelected = new Set(s.selectedFiles)
      const nextDeselected = new Set(s.userDeselected)
      // A range can sweep over a dirty submodule; it can never be included.
      const blocked = new Set(
        s.status.files.filter((f) => !isCommittable(f)).map((f) => f.path),
      )
      for (const p of paths) {
        if (include && !blocked.has(p)) {
          nextSelected.add(p)
          nextDeselected.delete(p)
        } else if (!include) {
          nextSelected.delete(p)
          nextDeselected.add(p)
        }
      }
      return { ...s, selectedFiles: nextSelected, userDeselected: nextDeselected }
    })
  }

  function handleFileToggle(file: FileEntry) {
    // The row's checkbox is disabled for dirty submodules; ignore any toggle
    // that still reaches here (e.g. a programmatic call) so it stays excluded.
    if (!isCommittable(file)) return
    repoState.update((s) => {
      const nextSelected = new Set(s.selectedFiles)
      const nextDeselected = new Set(s.userDeselected)
      if (nextSelected.has(file.path)) {
        nextSelected.delete(file.path)
        nextDeselected.add(file.path)
      } else {
        nextSelected.add(file.path)
        nextDeselected.delete(file.path)
      }
      return { ...s, selectedFiles: nextSelected, userDeselected: nextDeselected }
    })
  }

  // ---- Changes-tab context menu --------------------------------------------
  // Files pending a discard confirmation; null when the dialog is closed.
  let discardTarget = $state<FileEntry[] | null>(null)
  // What discarding them would actually do, as core decides it — the same
  // decision the discard itself runs on, so the dialog can't promise something
  // the action then doesn't do. Null until the answer arrives.
  let discardPlan = $state<DiscardPlan | null>(null)
  let isDiscarding = $state(false)

  // Run a side-effect-only file action (copy / reveal / open), surfacing any
  // failure through the shared error modal. No-op without an open repo.
  function runFileAction(fn: (repoPath: string) => Promise<void>): void {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    fn(repoPath).catch((error) => {
      repoState.update((s) => ({ ...s, error: String(error) }))
    })
  }

  async function ignoreFiles(append: (repoPath: string) => Promise<void>): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await append(repoPath)
      // The newly-ignored untracked file drops out of the changes list.
      await refreshStatus({ silent: true })
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  function requestDiscard(files: FileEntry[]): void {
    if (files.length === 0) return
    discardTarget = files
    discardPlan = null
    const repoPath = $appState.repoPath
    if (!repoPath) return
    gitApi
      .classifyDiscard(repoPath, files)
      .then((plan) => {
        // Ignore an answer about a dialog the user already dismissed.
        if (discardTarget === files) discardPlan = plan
      })
      .catch(() => {})
  }

  async function confirmDiscard(): Promise<void> {
    const repoPath = $appState.repoPath
    const files = discardTarget
    if (!repoPath || !files) return
    isDiscarding = true
    try {
      await gitApi.discardFiles(repoPath, files)
      discardTarget = null
      discardPlan = null
      // refreshStatus prunes the discarded files from the list / active diff.
      await refreshStatus({ silent: true })
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isDiscarding = false
    }
  }

  function cancelDiscard(): void {
    if (isDiscarding) return
    discardTarget = null
    discardPlan = null
  }

  // Intents raised by FileList's right-click menu. Repo path + refresh + the
  // confirm dialog live here; FileList only decides what to show.
  const fileContextActions: FileContextActions = {
    discard: requestDiscard,
    ignoreFile: (file) => void ignoreFiles((repo) => fileActions.ignoreFile(repo, file)),
    ignoreExtension: (ext) => void ignoreFiles((repo) => fileActions.ignoreExtension(repo, ext)),
    copyPath: (file) => runFileAction((repo) => fileActions.copyAbsolutePath(repo, file)),
    copyRelativePath: (file) => runFileAction(() => fileActions.copyRelativePath(file)),
    reveal: (file) => runFileAction((repo) => fileActions.revealFile(repo, file)),
    openWithDefault: (file) => runFileAction((repo) => fileActions.openWithDefault(repo, file)),
  }

  async function handleSwitchRepo(repo: string) {
    if (!repo || repo === $appState.repoPath) {
      showRepos = false
      return
    }
    showRepos = false
    lastHeadSha = null
    // The streak describes one repository's readability; carrying it across a
    // switch would let two failures on the old repo plus one on the new raise
    // the new one's banner. (`resetRepoState` clears `pollError` itself.)
    quietFailureStreak = 0
    resetRepoState()
    appState.update((s) => ({ ...s, repoPath: repo }))
    await patchReposState({ last_opened_repo: repo })
    // Promote to the front of the recents list (re-tiers the background sync)
    // and fetch it now — "open a repo" should always pull its latest counts,
    // including for the untiered long tail.
    recordRecentRepo(repo)
    repoSyncScheduler.syncOnSwitch(repo)
    try {
      await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
      const cfg = $config
      const intervalMs = cfg?.auto_fetch ? cfg.fetch_interval_ms || 30000 : 0
      startAutoFetch(intervalMs)
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  // A `leogit <dir>` invocation reached the already-running app (forwarded by
  // the single-instance plugin as an `open-repo` event), or a folder was just
  // initialised from App's prompt. Make the repo selectable — it may live
  // outside the scan paths — then switch to it. Re-running `leogit .` on the
  // open repo is a no-op beyond the window focus the backend already did.
  //
  // Exported so App can hand off a freshly created repo: only this component
  // can reset the open repo's view state, and it is already mounted.
  export async function openExternalRepo(path: string) {
    if (!path) return
    console.log('[launch] open-repo event — switching to:', path)
    if (!$appState.repos.includes(path)) {
      appState.update((s) => ({ ...s, repos: [...s.repos, path] }))
    }
    await handleSwitchRepo(path)
  }

  // Open the Clone dialog from the repo dropdown, seeding its destination from
  // the shared rule (last-used clone folder → first scan path → ~/Dev).
  async function openClone() {
    showRepos = false
    cloneDefaultDir = await resolveCloneDefaultDir()
    showClone = true
  }

  // A clone finished: remember where it landed, make it selectable, and open it.
  async function handleCloned(repoPath: string, parentDir: string) {
    showClone = false
    await rememberCloneDir(parentDir)
    appState.update((s) => ({
      ...s,
      repos: s.repos.includes(repoPath) ? s.repos : [...s.repos, repoPath],
    }))
    await handleSwitchRepo(repoPath)
  }

  /**
   * Show the repo dropdown, re-walking first so a repo cloned from a terminal
   * or a folder just added to the scan paths is in the list the user is about
   * to read. Not awaited: the list on screen is already usable, and the walk
   * publishes into it when it lands.
   */
  function openRepos() {
    showRepos = true
    void rediscoverRepos()
  }

  /**
   * Settings closed. The scan paths are what discovery walks, so re-walk — the
   * main phase used to need a restart for a scan-path edit to mean anything,
   * while the picker phase re-walked immediately: the same setting behaving two
   * different ways depending on where you opened it from.
   */
  function closeSettings() {
    showSettings = false
    void rediscoverRepos()
  }

  async function handleSwitchBranch(branch: string) {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.switchBranch(repoPath, branch)
      showBranches = false
      await refreshStatus()
      await refreshBranches()
      await handleCommitted()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  async function handleCreateBranch(name: string) {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.createBranch(repoPath, name, '')
      await gitApi.switchBranch(repoPath, name)
      showBranches = false
      await refreshStatus()
      await refreshBranches()
      await handleCommitted()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  async function handleDeleteBranch(name: string) {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.deleteBranch(repoPath, name)
      await refreshBranches()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  async function handleMerge() {
    const repoPath = $appState.repoPath
    if (!repoPath || !mergeTarget) return
    try {
      const result = await gitApi.mergeBranch(repoPath, mergeTarget)
      if (!result.success && result.error_message) {
        repoState.update((s) => ({ ...s, error: result.error_message }))
      }
      showMerge = false
      await refreshStatus()
      await handleCommitted()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  async function handleSquashMerge() {
    const repoPath = $appState.repoPath
    if (!repoPath || !mergeTarget) return
    try {
      const result = await gitApi.mergeSquash(repoPath, mergeTarget)
      if (result.success) {
        await gitApi.commitSquashMerge(repoPath)
      } else if (result.error_message) {
        repoState.update((s) => ({ ...s, error: result.error_message }))
      }
      showMerge = false
      await refreshStatus()
      await handleCommitted()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  function dismissError() {
    repoState.update((s) => ({ ...s, error: undefined }))
  }

  function toggleTerminalMinimize() {
    if (!terminalExpanded && terminalSessionId === 0) {
      terminalSessionId = 1
    }
    terminalExpanded = !terminalExpanded
  }

  function newTerminalSession() {
    // Clear the old label so the header can't show the previous session's
    // shell while the new one is still starting.
    activeShellLabel = ''
    terminalSessionId += 1
    terminalExpanded = true
  }

  function killTerminalSession() {
    activeShellLabel = ''
    terminalSessionId = 0
    terminalExpanded = false
  }

  function handleKeyDown(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null
    const inField = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement
    const meta = e.ctrlKey || e.metaKey

    // The terminal owns its keys. The panel's own toggle is the single
    // exception — xterm hands it back to us via `attachCustomKeyEventHandler`,
    // and it has to be handled *before* the `inField` bail below, because
    // xterm's input sink is a <textarea>. See `utils/keyboard.ts`.
    if (isFromTerminal(e)) {
      if (meta && e.key === '`') {
        e.preventDefault()
        toggleTerminalMinimize()
      }
      return
    }

    if (e.key === 'Escape') {
      // Escape never dismisses a clone in flight — hiding the dialog wouldn't
      // cancel the clone, just orphan its progress bar and eventual error.
      const cloneDismissable = showClone && !(cloneOverlay?.isBusy() ?? false)
      if (showRepos || cloneDismissable || showBranches || showSettings || showHelp || showMerge) {
        e.preventDefault()
        // Escape closes Settings the same way its own close button does — a
        // scan-path edit must re-walk however the dialog was dismissed.
        if (showSettings) closeSettings()
        showRepos = showBranches = showHelp = showMerge = false
        if (cloneDismissable) showClone = false
        return
      }
    }

    if (inField) return

    if (meta && e.key === '`') {
      e.preventDefault()
      toggleTerminalMinimize()
    } else if (meta && e.key === 'b') {
      e.preventDefault()
      showBranches = !showBranches
    } else if (e.key === '?' && !meta) {
      e.preventDefault()
      showHelp = !showHelp
    } else if (meta && e.key === ',') {
      e.preventDefault()
      if (showSettings) closeSettings()
      else showSettings = true
    } else if (meta && e.key === 'r') {
      e.preventDefault()
      refreshStatus()
    } else if (meta && e.key === 'l') {
      e.preventDefault()
      repoState.update((s) => ({ ...s, activeTab: s.activeTab === 'changes' ? 'history' : 'changes' }))
    }
  }

  async function initialize() {
    await refreshConfig()
    // Seed the persisted sort-mode toggles and recents list before either
    // picker opens. Awaited so recordRecentRepo below prepends to the hydrated
    // list rather than racing the hydration that would otherwise clobber it.
    await hydrateReposState()
    // The repo we launched into counts as the most-recent open.
    const repoPath = $appState.repoPath
    if (repoPath) recordRecentRepo(repoPath)
    await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
    startStatusPolling()
    const cfg = $config
    const intervalMs = cfg?.auto_fetch ? cfg.fetch_interval_ms || 30000 : 0
    startAutoFetch(intervalMs)
    // Kick one immediate fetch of the open repo so the Pull "behind" badge
    // resolves within a second of launch instead of waiting up to a full
    // auto-fetch interval (and at all when auto-fetch is off). Non-blocking so
    // it never delays first paint. Mirrors the fetch-on-refocus behaviour.
    void performAutoFetch()
    // Background pull/push badges for the other recent repos in the picker.
    repoSyncScheduler.start()
  }

  $effect(() => {
    if ($repoState.activeTab === 'history' && !$repoState.log.loaded) {
      loadInitialLog()
    }
  })

  // When hide_whitespace toggles in settings, reload the active diff
  let lastHideWhitespace = $state<boolean | undefined>(undefined)
  $effect(() => {
    const cfg = $config
    if (!cfg) return
    if (lastHideWhitespace === undefined) {
      lastHideWhitespace = cfg.hide_whitespace
      return
    }
    if (cfg.hide_whitespace !== lastHideWhitespace) {
      lastHideWhitespace = cfg.hide_whitespace
      if ($repoState.activeFile) loadDiffForFile($repoState.activeFile)
    }
  })

  // Kill terminal PTY on project change so we don't leak shells from prior repos.
  let lastTerminalRepoPath = $state<string | undefined>(undefined)
  $effect(() => {
    const path = $appState.repoPath
    if (lastTerminalRepoPath === undefined) {
      lastTerminalRepoPath = path
      return
    }
    if (path !== lastTerminalRepoPath) {
      lastTerminalRepoPath = path
      activeShellLabel = ''
      terminalSessionId = 0
      terminalExpanded = false
    }
  })

  let unlistenOpenRepo: (() => void) | null = null
  let teardownConnectivity: (() => void) | null = null

  onMount(() => {
    initialize().catch(console.error)
    // Live `leogit <dir>` switches while the app is open (see openExternalRepo).
    // A target that isn't a repository yet is App's to handle — it owns the
    // "create a repository here?" prompt for every phase.
    listen<LaunchTarget>('open-repo', (e) => {
      if (e.payload?.is_repo) openExternalRepo(e.payload.path).catch(console.error)
    }).then((u) => {
      unlistenOpenRepo = u
    })
    document.addEventListener('visibilitychange', handleVisibilityChange)
    window.addEventListener('focus', handleWindowFocus)
    document.addEventListener('focusin', handleFocusEvent)
    document.addEventListener('focusout', handleFocusEvent)
    window.addEventListener('keydown', handleKeyDown)
    // The moment the OS reports connectivity back, refresh the active repo and
    // the top picker tier immediately instead of waiting out the backoff window.
    teardownConnectivity = initConnectivity(() => {
      if ($appState.phase !== 'main') return
      void performAutoFetch()
      repoSyncScheduler.refocusSync()
    })

    return () => {
      if (statusInterval) clearInterval(statusInterval)
      if (fetchInterval) clearInterval(fetchInterval)
      repoSyncScheduler.stop()
      teardownConnectivity?.()
      unlistenOpenRepo?.()
      document.removeEventListener('visibilitychange', handleVisibilityChange)
      window.removeEventListener('focus', handleWindowFocus)
      document.removeEventListener('focusin', handleFocusEvent)
      document.removeEventListener('focusout', handleFocusEvent)
      window.removeEventListener('keydown', handleKeyDown)
    }
  })
</script>

<div class="main-layout" style="--sidebar-width: {sidebarWidth}px;">
  <div class="sidebar">
    <TabBar />
    <!--
      Both tab panes stay mounted and toggle via CSS so CommitMessage retains
      its in-progress draft (summary / description / co-authors) when the user
      switches to History and back. CommitList also keeps its scroll position.
    -->
    <div class="tab-pane" class:active={$repoState.activeTab === 'changes'}>
      <div class="file-list-container">
        <FileList
          files={$repoState.status.files}
          selectedFiles={$repoState.selectedFiles}
          activeFile={$repoState.activeFile}
          contextActions={fileContextActions}
          onActivate={handleFileActivate}
          onToggle={handleFileToggle}
          onToggleAll={handleToggleAll}
          onBulkToggle={handleBulkToggle}
        />
      </div>
      <div class="commit-section" style="height: {commitHeight}px;">
        <div
          class="commit-resize-handle"
          onmousedown={startCommitResize}
          onkeydown={handleCommitKey}
          role="slider"
          tabindex="0"
          aria-orientation="horizontal"
          aria-label="Resize commit section"
          aria-valuenow={commitHeight}
          aria-valuemin={COMMIT_MIN}
          aria-valuemax={COMMIT_MAX}
        ></div>
        <CommitMessage onCommitted={handleCommitted} onStopAmending={handleStopAmending} />
      </div>
    </div>
    <div class="tab-pane" class:active={$repoState.activeTab === 'history'}>
      <div class="commit-list-container">
        <CommitList
          commits={$repoState.log.commits}
          selectedSha={$repoState.activeCommit?.sha || null}
          unpushedShas={$repoState.status.unpushedShas}
          hasResolvedUpstream={$repoState.status.upstream !== ''}
          headSha={$repoState.status.headSha}
          windowStartOffset={$repoState.log.windowStartOffset}
          resetSeq={$repoState.log.resetSeq}
          loaded={$repoState.log.loaded}
          onSelect={loadCommitFiles}
          onLoadMore={loadMoreCommits}
          onLoadEarlier={loadEarlierCommits}
          onAmendCommit={handleStartAmending}
          onUndoCommit={handleUndoCommit}
          onCheckoutCommit={handleCheckoutCommit}
        />
      </div>
    </div>
  </div>

  <div
    class="sidebar-resize-handle"
    onmousedown={startSidebarResize}
    onkeydown={handleSidebarKey}
    role="slider"
    tabindex="0"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    aria-valuenow={sidebarWidth}
    aria-valuemin={SIDEBAR_MIN}
    aria-valuemax={SIDEBAR_MAX}
  ></div>

  <div class="main-content">
    <Header
      onOpenRepos={openRepos}
      onOpenBranches={() => (showBranches = true)}
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
      onRefresh={refreshStatus}
    />

    <!--
      Background failures never take the window. The poll owns this strip: it
      appears after a streak of failed ticks (the repo went away) and clears
      itself the moment a tick succeeds, so the last good snapshot stays
      readable behind it instead of being hidden by a modal the user has to
      dismiss on every tick. User-initiated failures still go to ErrorModal.
    -->
    {#if $repoState.pollError}
      <div class="poll-banner" role="status">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M8 2.5 14.5 13.5h-13z" />
          <line x1="8" y1="6.5" x2="8" y2="9.5" />
          <circle cx="8" cy="11.6" r="0.6" fill="currentColor" stroke="none" />
        </svg>
        <span class="poll-banner-text">
          Can't read this repository — it may have been moved, deleted, or unmounted.
        </span>
        <span class="poll-banner-detail">{$repoState.pollError}</span>
      </div>
    {/if}

    <div class="content-area">
      {#if $repoState.activeTab === 'changes'}
        {#if $repoState.isDiffLoadingSlow}
          <div class="diff-empty">Loading diff…</div>
        {:else if $repoState.activeFile?.submodule_dirty}
          <div class="diff-empty submodule-changes">
            <p class="submodule-title">Submodule changes</p>
            <p class="muted">
              This submodule has modified content that hasn't been committed. Those changes
              must be committed inside the submodule before they can be part of this
              repository.
            </p>
          </div>
        {:else if hasRenderableDiff($repoState.activeFileDiff)}
          <DiffViewer
            diff={$repoState.activeFileDiff!}
            selection={null}
            blobSource={{ kind: 'workingTree', repoPath: $appState.repoPath }}
            showSelection={false}
            syntaxHighlighting={$config?.syntax_highlighting ?? true}
            sideBySide={$config?.side_by_side_diff ?? false}
            tabSize={$config?.tab_size ?? 4}
          />
        {:else if $repoState.activeFileDiff?.size_guard}
          <!-- Withheld rather than empty: rendering it would be slow, so the
               pane explains and offers it instead of hanging on it. -->
          <div class="diff-empty">
            <p>Large diff</p>
            <p class="muted">{sizeGuardCopy($repoState.activeFileDiff.size_guard!)}</p>
            <button class="show-anyway" onclick={() => showActiveDiffAnyway()}>
              Show diff anyway
            </button>
          </div>
        {:else if $repoState.activeFile}
          <!--
            A file IS selected but there is nothing to render. Core says which
            of the three unrelated reasons it is; falling through to the
            no-selection copy below told the user to select the file they had
            already selected. Stays blank while the fetch is in flight so a
            sub-threshold load doesn't flash this state on its way to the diff.
          -->
          <div class="diff-empty">
            {#if !$repoState.isDiffLoading}
              {@const copy = emptyDiffCopy($repoState.activeFileDiff)}
              <p>{copy.title}</p>
              <p class="muted">{copy.detail}</p>
            {/if}
          </div>
        {:else}
          <div class="diff-empty">
            {#if $repoState.status.files.length === 0}
              <p>No changes</p>
              <p class="muted">Working tree is clean</p>
            {:else}
              <p>Select a file to view its diff</p>
            {/if}
          </div>
        {/if}
      {:else if $repoState.activeCommit}
        <CommitDetail
          commit={$repoState.activeCommit}
          fileCount={$repoState.activeCommitFiles.length}
          stats={$repoState.activeCommitStats}
        />
        <div class="commit-body" style="--commit-files-width: {commitFilesWidth}px;">
          <div class="commit-files-pane">
            <FileList
              files={$repoState.activeCommitFiles}
              activeFile={$repoState.activeCommitFile}
              showCheckbox={false}
              onActivate={loadCommitFileDiff}
            />
          </div>
          <div
            class="commit-files-resize-handle"
            onmousedown={startCommitFilesResize}
            onkeydown={handleCommitFilesKey}
            role="slider"
            tabindex="0"
            aria-orientation="vertical"
            aria-label="Resize commit files pane"
            aria-valuenow={commitFilesWidth}
            aria-valuemin={COMMIT_FILES_MIN}
            aria-valuemax={COMMIT_FILES_MAX}
          ></div>
          <div class="commit-diff-pane">
            {#if $repoState.isCommitDiffLoadingSlow}
              <div class="diff-empty">Loading diff…</div>
            {:else if hasRenderableDiff($repoState.activeCommitFileDiff)}
              <DiffViewer
                diff={$repoState.activeCommitFileDiff!}
                selection={null}
                blobSource={$repoState.activeCommit
                  ? {
                      kind: 'commit',
                      repoPath: $appState.repoPath,
                      sha: $repoState.activeCommit.sha,
                    }
                  : null}
                showSelection={false}
                syntaxHighlighting={$config?.syntax_highlighting ?? true}
                sideBySide={$config?.side_by_side_diff ?? false}
                tabSize={$config?.tab_size ?? 4}
              />
            {:else if $repoState.activeCommitFileDiff?.size_guard}
              <div class="diff-empty">
                <p>Large diff</p>
                <p class="muted">{sizeGuardCopy($repoState.activeCommitFileDiff.size_guard!)}</p>
                <button class="show-anyway" onclick={() => showActiveCommitDiffAnyway()}>
                  Show diff anyway
                </button>
              </div>
            {:else if $repoState.activeCommitFile}
              <!-- Same split as the changes pane above: selected, but nothing
                   to render, and core names which reason. -->
              <div class="diff-empty">
                {#if !$repoState.isCommitDiffLoading}
                  {@const copy = emptyDiffCopy($repoState.activeCommitFileDiff)}
                  <p>{copy.title}</p>
                  <p class="muted">{copy.detail}</p>
                {/if}
              </div>
            {:else}
              <div class="diff-empty">
                <p>Select a file to view its diff</p>
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="diff-empty">
          <p>Select a commit to view its changes</p>
        </div>
      {/if}
    </div>

    {#if $appState.repoPath}
      <div class="terminal-section" class:collapsed={!terminalExpanded}>
        <div class="terminal-header">
          <button
            class="terminal-label"
            onclick={toggleTerminalMinimize}
            title={terminalExpanded ? 'Minimize terminal (Ctrl+`)' : 'Expand terminal (Ctrl+`)'}
            aria-label={terminalExpanded ? 'Minimize terminal' : 'Expand terminal'}
          >
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <polyline points="4,6 7,8 4,10" />
              <line x1="8.5" y1="11" x2="12" y2="11" />
            </svg>
            {#if terminalSessionId > 0 && activeShellLabel}
              <span class="shell-name">{activeShellLabel}</span>
            {/if}
          </button>
          <div class="terminal-controls">
            <button
              class="terminal-control-button"
              onclick={newTerminalSession}
              title="New terminal session"
              aria-label="New terminal session"
            >
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
                <line x1="8" y1="3" x2="8" y2="13" />
                <line x1="3" y1="8" x2="13" y2="8" />
              </svg>
            </button>
            <button
              class="terminal-control-button"
              onclick={toggleTerminalMinimize}
              title={terminalExpanded ? 'Minimize terminal (Ctrl+`)' : 'Expand terminal (Ctrl+`)'}
              aria-label={terminalExpanded ? 'Minimize terminal' : 'Expand terminal'}
            >
              {#if terminalExpanded}
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
                  <line x1="3" y1="8" x2="13" y2="8" />
                </svg>
              {:else}
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <polyline points="3,10 8,5 13,10" />
                </svg>
              {/if}
            </button>
            <button
              class="terminal-control-button close-button"
              onclick={killTerminalSession}
              title="Close terminal"
              aria-label="Close terminal"
              disabled={terminalSessionId === 0}
            >
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
                <line x1="4" y1="4" x2="12" y2="12" />
                <line x1="12" y1="4" x2="4" y2="12" />
              </svg>
            </button>
          </div>
        </div>
        {#if terminalSessionId > 0}
          <div class="terminal-container">
            {#key `${$appState.repoPath}:${terminalSessionId}`}
              <Terminal
                repoPath={$appState.repoPath}
                shellId={$config?.terminal_shell}
                expanded={terminalExpanded}
                onExit={killTerminalSession}
                onShellResolved={(label) => (activeShellLabel = label)}
              />
            {/key}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if showRepos}
    <div
      class="overlay-backdrop"
      role="presentation"
      onclick={(e) => {
        if (e.target === e.currentTarget) showRepos = false
      }}
    >
      <div class="overlay-content" role="dialog" aria-modal="true" tabindex="-1">
        <RepoDropdown
          repos={$appState.repos}
          currentRepo={$appState.repoPath}
          onSelect={handleSwitchRepo}
          onClone={openClone}
          onOpenSettings={() => { showRepos = false; showSettings = true }}
        />
      </div>
    </div>
  {/if}

  <CloneOverlay
    bind:this={cloneOverlay}
    isOpen={showClone}
    defaultDir={cloneDefaultDir}
    onClose={() => (showClone = false)}
    onCloned={handleCloned}
  />

  {#if showBranches}
    <div
      class="overlay-backdrop"
      role="presentation"
      onclick={(e) => {
        if (e.target === e.currentTarget) showBranches = false
      }}
    >
      <div class="overlay-content" role="dialog" aria-modal="true" tabindex="-1">
        <BranchDropdown
          branches={$repoState.branches}
          currentBranch={$repoState.status.branch}
          onSwitch={handleSwitchBranch}
          onCreate={handleCreateBranch}
          onDelete={handleDeleteBranch}
        />
      </div>
    </div>
  {/if}

  {#if showMerge}
    <MergeOverlay
      sourceBranch={mergeTarget}
      targetBranch={$repoState.status.branch}
      onMerge={handleMerge}
      onSquashMerge={handleSquashMerge}
      onAbort={() => (showMerge = false)}
    />
  {/if}

  <SettingsOverlay isOpen={showSettings} onClose={closeSettings} />
  <HelpOverlay isOpen={showHelp} onClose={() => (showHelp = false)} />

  {#if discardTarget}
    <DiscardConfirm
      files={discardTarget}
      plan={discardPlan}
      {isDiscarding}
      onConfirm={confirmDiscard}
      onCancel={cancelDiscard}
    />
  {/if}

  {#if checkoutTarget}
    <CheckoutCommitConfirm
      commit={checkoutTarget}
      {isCheckingOut}
      onConfirm={confirmCheckout}
      onCancel={cancelCheckout}
    />
  {/if}

  {#if $repoState.error}
    <ErrorModal
      title="Error"
      message={$repoState.error}
      onDismiss={dismissError}
    />
  {/if}
</div>

<style>
  .main-layout {
    display: grid;
    grid-template-columns: var(--sidebar-width, 320px) 1px 1fr;
    width: 100%;
    height: 100vh;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    overflow: hidden;
    min-height: 0;
  }

  .sidebar-resize-handle {
    position: relative;
    width: 1px;
    background: var(--border-inactive);
    cursor: col-resize;
    user-select: none;
    transition: background 120ms ease;
  }

  .sidebar-resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    right: -3px;
    z-index: 10;
  }

  .sidebar-resize-handle:hover,
  .sidebar-resize-handle:active {
    background: var(--border-active);
  }

  .commit-section {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 0;
    position: relative;
  }

  .commit-resize-handle {
    height: 4px;
    background: transparent;
    cursor: row-resize;
    user-select: none;
    flex-shrink: 0;
    border-top: 1px solid var(--border-inactive);
    margin-bottom: -1px;
    position: relative;
    z-index: 2;
  }

  .commit-resize-handle::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: -3px;
    bottom: -3px;
  }

  .commit-resize-handle:hover,
  .commit-resize-handle:active {
    background: var(--border-active);
  }

  .file-list-container,
  .commit-list-container {
    flex: 1;
    overflow: hidden;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /*
    Both tab subtrees stay mounted (so CommitMessage doesn't drop its draft
    when the user peeks at History). Only the active one renders.
  */
  .tab-pane {
    display: none;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .tab-pane.active {
    display: flex;
  }

  .main-content {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    background: var(--bg-primary);
  }

  .content-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  /* Poll-failure strip: one line under the header, above the content, so the
     data behind it stays visible. Deliberately not a toast and not a modal —
     the condition persists, so it must be able to sit there. */
  .poll-banner {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-inactive);
    background: color-mix(in srgb, var(--status-yellow) 12%, transparent);
    color: var(--text-primary);
    font-size: 12px;
  }

  .poll-banner svg {
    flex-shrink: 0;
    align-self: center;
    color: var(--status-yellow);
  }

  .poll-banner-text {
    flex-shrink: 0;
  }

  .poll-banner-detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    user-select: text;
  }

  .commit-body {
    flex: 1;
    display: grid;
    grid-template-columns: var(--commit-files-width, 280px) 1px 1fr;
    min-height: 0;
    overflow: hidden;
  }

  .commit-files-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-primary);
  }

  .commit-files-resize-handle {
    position: relative;
    background: var(--border-inactive);
    cursor: col-resize;
    z-index: 5;
  }

  .commit-files-resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    right: -3px;
  }

  .commit-files-resize-handle:hover,
  .commit-files-resize-handle:active {
    background: var(--border-active);
  }

  .commit-diff-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .diff-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--text-secondary);
    background: var(--bg-primary);
    font-size: 13px;
  }

  .diff-empty .muted {
    color: var(--text-faint);
    font-size: 12px;
    /* A pane-wide single line reads as a banner; wrap the explanatory ones to
       a column under their title. */
    max-width: 420px;
    text-align: center;
    line-height: 1.5;
  }

  /* Submodule whose inner working tree is dirty but pointer hasn't moved: the
     raw diff is just an opaque `Subproject commit …-dirty` line, so we explain
     it instead, mirroring the checkbox being disabled in the file list. */
  .submodule-changes {
    padding: 0 32px;
  }

  .submodule-changes .submodule-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .submodule-changes .muted {
    max-width: 420px;
    text-align: center;
    line-height: 1.5;
  }

  .terminal-section {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    height: 280px;
    border-top: 1px solid var(--border-inactive);
    background: #000000;
  }

  .terminal-section.collapsed {
    height: auto;
    background: var(--bg-secondary);
  }

  .terminal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 26px;
    padding: 0 6px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-inactive);
    flex-shrink: 0;
  }

  .terminal-section.collapsed .terminal-header {
    border-bottom: none;
  }

  .terminal-section.collapsed .terminal-container {
    display: none;
  }

  .terminal-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 20px;
    padding: 0 6px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: background 100ms ease, color 100ms ease;
  }

  .terminal-label:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  /* Which shell is running. Secondary to the panel controls, so it stays at
     the muted weight even while the label button is hovered. */
  .shell-name {
    font-size: 10px;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  .terminal-controls {
    display: flex;
    gap: 1px;
  }

  .terminal-control-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    line-height: 0;
    transition: background 100ms ease, color 100ms ease;
  }

  .terminal-control-button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .terminal-control-button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .terminal-control-button.close-button:hover:not(:disabled) {
    background: var(--status-red);
    color: #ffffff;
  }

  .terminal-container {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: #000000;
  }

  .overlay-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 60px;
    z-index: 1000;
  }

  .overlay-content {
    background: transparent;
  }
</style>

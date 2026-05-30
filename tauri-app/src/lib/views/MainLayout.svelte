<script lang="ts">
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { repoState, resetRepoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { config, refreshConfig } from '$lib/stores/config'
  import { gitApi, diffApi, configApi, type FileEntry, type CommitInfo } from '$lib/api/commands'

  import Header from '$lib/components/Header.svelte'
  import TabBar from '$lib/components/TabBar.svelte'
  import FileList from '$lib/components/FileList.svelte'
  import CommitMessage from '$lib/components/CommitMessage.svelte'
  import CommitList from '$lib/components/CommitList.svelte'
  import DiffViewer from '$lib/components/DiffViewer.svelte'
  import Terminal from '$lib/components/Terminal.svelte'
  import CommitDetail from '$lib/views/CommitDetail.svelte'
  import BranchDropdown from '$lib/views/BranchDropdown.svelte'
  import RepoDropdown from '$lib/views/RepoDropdown.svelte'
  import MergeOverlay from '$lib/views/MergeOverlay.svelte'
  import SettingsOverlay from '$lib/views/SettingsOverlay.svelte'
  import HelpOverlay from '$lib/views/HelpOverlay.svelte'
  import ErrorModal from '$lib/components/ErrorModal.svelte'

  let terminalExpanded = $state(false)
  let terminalSessionId = $state(0) // 0 = no active PTY; >0 = key for the mounted Terminal
  let showRepos = $state(false)
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

  async function refreshStatus(opts: { silent?: boolean } = {}): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const status = await gitApi.getStatus(repoPath)
      const isMerging = await gitApi.isMerging(repoPath).catch(() => false)
      repoState.update((s) => {
        const presentPaths = new Set(status.files.map((f) => f.path))
        const nextSelected = new Set<string>()
        for (const f of status.files) {
          if (!s.userDeselected.has(f.path)) nextSelected.add(f.path)
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
          status: {
            branch: status.branch,
            upstream: status.upstream,
            hasUpstream: status.has_upstream,
            ahead: status.ahead,
            behind: status.behind,
            files: status.files,
            isMerging,
            unpushedShas: new Set(status.unpushed_shas ?? []),
          },
          selectedFiles: nextSelected,
          userDeselected: nextDeselected,
          activeFile: activeFileGone ? null : s.activeFile,
          activeFileDiff: activeFileGone ? null : s.activeFileDiff,
          isDiffLoading: activeFileGone ? false : s.isDiffLoading,
          error: opts.silent ? s.error : undefined,
        }
      })
    } catch (error) {
      if (!opts.silent) {
        repoState.update((s) => ({ ...s, error: String(error) }))
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
          },
        }))
        return
      }
      repoState.update((s) => ({
        ...s,
        log: {
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

  async function loadDiffForFile(file: FileEntry | null): Promise<void> {
    // Re-activating the same file that's already on screen (or already
    // being fetched) is a no-op — skip the refetch so arrow scrolls past
    // and back don't churn the pane.
    const current = get(repoState)
    if (
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
      const raw = cfg?.hide_whitespace
        ? await gitApi.getDiffWhitespaceIgnored(repoPath, file)
        : await gitApi.getDiff(repoPath, file)
      const parsed = await diffApi.parseDiff(raw)
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

  async function loadCommitFiles(commit: CommitInfo | null): Promise<void> {
    repoState.update((s) => ({
      ...s,
      activeCommit: commit,
      activeCommitFiles: [],
      activeCommitFile: null,
      activeCommitFileDiff: null,
    }))
    if (!commit) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const files = await gitApi.getCommitFiles(repoPath, commit.sha)
      repoState.update((s) => ({ ...s, activeCommitFiles: files }))
      if (files.length > 0) {
        loadCommitFileDiff(files[0])
      }
    } catch {}
  }

  async function loadCommitFileDiff(file: FileEntry | null): Promise<void> {
    const current = get(repoState)
    if (
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
      const raw = await gitApi.getCommitDiff(repoPath, commit.sha, file.path)
      const parsed = await diffApi.parseDiff(raw)
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

  async function performAutoFetch(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const remote = await gitApi.getRemote(repoPath)
      await gitApi.fetch(repoPath, remote)
      await refreshStatus({ silent: true })
    } catch {}
  }

  function startStatusPolling(): void {
    if (statusInterval) clearInterval(statusInterval)
    statusInterval = setInterval(() => {
      if ($appState.phase !== 'main') return
      refreshStatus({ silent: true })
      pollHeadSha()
    }, 2000)
  }

  function startAutoFetch(intervalMs: number): void {
    if (fetchInterval) clearInterval(fetchInterval)
    if (intervalMs <= 0) return
    fetchInterval = setInterval(() => {
      if ($appState.phase !== 'main' || userTyping) return
      performAutoFetch()
    }, intervalMs)
  }

  function handleVisibilityChange(): void {
    if (!document.hidden && $appState.phase === 'main') {
      refreshStatus({ silent: true })
      pollHeadSha()
    }
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

  // Parse the Co-Authored-By trailers off a commit and split the body.
  // Mirrors CommitMessage.svelte's helpers — kept local to avoid coupling.
  function splitCoAuthors(commit: CommitInfo): {
    body: string
    coAuthors: string[]
  } {
    const coAuthors: string[] = []
    for (const raw of commit.trailers) {
      const m = raw.match(/^\s*Co-Authored-By:\s*(.+?)\s*$/i)
      if (m && m[1]) coAuthors.push(m[1])
    }
    const body = commit.body
      .split('\n')
      .filter((line) => !/^\s*Co-Authored-By:/i.test(line))
      .join('\n')
      .trimEnd()
    return { body, coAuthors }
  }

  async function handleUndoCommit(commit: CommitInfo): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.undoLastCommit(repoPath)
      const { body, coAuthors } = splitCoAuthors(commit)
      // Set the seed BEFORE refresh so the composer prefills as soon as the
      // tab switches over. Also defensively clear amend mode in case the
      // undone commit happened to be the one the user was amending.
      repoState.update((s) => ({
        ...s,
        commitToAmend: null,
        restoreMessage: {
          summary: commit.summary,
          description: body,
          coAuthors,
        },
        activeTab: 'changes',
      }))
      await handleCommitted()
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
          selectedFiles: new Set(s.status.files.map((f) => f.path)),
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
      for (const p of paths) {
        if (include) {
          nextSelected.add(p)
          nextDeselected.delete(p)
        } else {
          nextSelected.delete(p)
          nextDeselected.add(p)
        }
      }
      return { ...s, selectedFiles: nextSelected, userDeselected: nextDeselected }
    })
  }

  function handleFileToggle(file: FileEntry) {
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

  async function handleSwitchRepo(repo: string) {
    if (!repo || repo === $appState.repoPath) {
      showRepos = false
      return
    }
    showRepos = false
    lastHeadSha = null
    resetRepoState()
    appState.update((s) => ({ ...s, repoPath: repo }))
    try {
      await configApi.saveState({ last_opened_repo: repo })
    } catch {}
    try {
      await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
      const cfg = $config
      const intervalMs = cfg?.auto_fetch ? cfg.fetch_interval_ms || 30000 : 0
      startAutoFetch(intervalMs)
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
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
    terminalSessionId += 1
    terminalExpanded = true
  }

  function killTerminalSession() {
    terminalSessionId = 0
    terminalExpanded = false
  }

  function handleKeyDown(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null
    const inField = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement
    const meta = e.ctrlKey || e.metaKey

    if (e.key === 'Escape') {
      if (showRepos || showBranches || showSettings || showHelp || showMerge) {
        e.preventDefault()
        showRepos = showBranches = showSettings = showHelp = showMerge = false
        return
      }
    }

    if (inField) return

    if (e.key === '`' && !meta) {
      e.preventDefault()
      toggleTerminalMinimize()
    } else if (e.key === 'b' && !meta) {
      e.preventDefault()
      showBranches = !showBranches
    } else if (e.key === '?' && !meta) {
      e.preventDefault()
      showHelp = !showHelp
    } else if (e.key === ',' && !meta) {
      e.preventDefault()
      showSettings = !showSettings
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
    await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
    startStatusPolling()
    const cfg = $config
    const intervalMs = cfg?.auto_fetch ? cfg.fetch_interval_ms || 30000 : 0
    startAutoFetch(intervalMs)
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
      terminalSessionId = 0
      terminalExpanded = false
    }
  })

  onMount(() => {
    initialize().catch(console.error)
    document.addEventListener('visibilitychange', handleVisibilityChange)
    document.addEventListener('focusin', handleFocusEvent)
    document.addEventListener('focusout', handleFocusEvent)
    window.addEventListener('keydown', handleKeyDown)

    return () => {
      if (statusInterval) clearInterval(statusInterval)
      if (fetchInterval) clearInterval(fetchInterval)
      document.removeEventListener('visibilitychange', handleVisibilityChange)
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
          windowStartOffset={$repoState.log.windowStartOffset}
          onSelect={loadCommitFiles}
          onLoadMore={loadMoreCommits}
          onLoadEarlier={loadEarlierCommits}
          onAmendCommit={handleStartAmending}
          onUndoCommit={handleUndoCommit}
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
      onOpenRepos={() => (showRepos = true)}
      onOpenBranches={() => (showBranches = true)}
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
    />

    <div class="content-area">
      {#if $repoState.activeTab === 'changes'}
        {#if $repoState.isDiffLoadingSlow}
          <div class="diff-empty">Loading diff…</div>
        {:else if $repoState.activeFileDiff}
          <DiffViewer
            fileDiff={$repoState.activeFileDiff}
            selection={null}
            repoPath={$appState.repoPath}
            showSelection={false}
            syntaxHighlighting={$config?.syntax_highlighting ?? true}
            sideBySide={$config?.side_by_side_diff ?? false}
            tabSize={$config?.tab_size ?? 4}
            wrapLongLines={$config?.wrap_long_lines ?? true}
          />
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
            {:else if $repoState.activeCommitFileDiff}
              <DiffViewer
                fileDiff={$repoState.activeCommitFileDiff}
                selection={null}
                repoPath={$appState.repoPath}
                showSelection={false}
                syntaxHighlighting={$config?.syntax_highlighting ?? true}
                sideBySide={$config?.side_by_side_diff ?? false}
                tabSize={$config?.tab_size ?? 4}
              />
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
            title={terminalExpanded ? 'Minimize terminal (`)' : 'Expand terminal (`)'}
            aria-label={terminalExpanded ? 'Minimize terminal' : 'Expand terminal'}
          >
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <polyline points="4,6 7,8 4,10" />
              <line x1="8.5" y1="11" x2="12" y2="11" />
            </svg>
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
              title={terminalExpanded ? 'Minimize terminal (`)' : 'Expand terminal (`)'}
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
              <Terminal repoPath={$appState.repoPath} />
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
        />
      </div>
    </div>
  {/if}

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

  <SettingsOverlay isOpen={showSettings} onClose={() => (showSettings = false)} />
  <HelpOverlay isOpen={showHelp} onClose={() => (showHelp = false)} />

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

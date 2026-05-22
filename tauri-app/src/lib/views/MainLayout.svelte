<script lang="ts">
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { repoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { config, refreshConfig } from '$lib/stores/config'
  import { gitApi, diffApi, type FileEntry, type CommitInfo } from '$lib/api/commands'

  import Header from '$lib/components/Header.svelte'
  import TabBar from '$lib/components/TabBar.svelte'
  import FileList from '$lib/components/FileList.svelte'
  import CommitMessage from '$lib/components/CommitMessage.svelte'
  import CommitList from '$lib/components/CommitList.svelte'
  import DiffViewer from '$lib/components/DiffViewer.svelte'
  import Terminal from '$lib/components/Terminal.svelte'
  import CommitDetail from '$lib/views/CommitDetail.svelte'
  import BranchDropdown from '$lib/views/BranchDropdown.svelte'
  import MergeOverlay from '$lib/views/MergeOverlay.svelte'
  import SettingsOverlay from '$lib/views/SettingsOverlay.svelte'
  import HelpOverlay from '$lib/views/HelpOverlay.svelte'
  import ErrorModal from '$lib/components/ErrorModal.svelte'

  let terminalVisible = $state(false)
  let showBranches = $state(false)
  let showSettings = $state(false)
  let showHelp = $state(false)
  let showMerge = $state(false)
  let showPRs = $state(false)
  let mergeTarget = $state<string>('')

  let statusInterval: ReturnType<typeof setInterval> | null = null
  let fetchInterval: ReturnType<typeof setInterval> | null = null
  let userTyping = $state(false)

  const PAGE_SIZE = 50

  const SIDEBAR_MIN = 280
  const SIDEBAR_MAX = 640
  const COMMIT_MIN = 140
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
  let commitHeight = $state(loadStoredNumber('leogit:commitHeight', 200, COMMIT_MIN, COMMIT_MAX))
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
          },
          selectedFiles: nextSelected,
          userDeselected: nextDeselected,
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
        },
      }))
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  async function loadMoreCommits(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    if (!current.log.hasMore || current.isLoading) return

    repoState.update((s) => ({ ...s, isLoading: true }))
    try {
      const commits = await gitApi.getLog(repoPath, PAGE_SIZE, current.log.commits.length)
      repoState.update((s) => ({
        ...s,
        log: {
          commits: [...s.log.commits, ...commits],
          hasMore: commits.length === PAGE_SIZE,
          loaded: true,
        },
        isLoading: false,
      }))
    } catch (error) {
      repoState.update((s) => ({ ...s, isLoading: false, error: String(error) }))
    }
  }

  async function loadDiffForFile(file: FileEntry | null): Promise<void> {
    repoState.update((s) => ({ ...s, activeFile: file, activeFileDiff: null, isDiffLoading: file !== null }))
    if (!file) return

    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const cfg = $config
      const raw = cfg?.hide_whitespace
        ? await gitApi.getDiffWhitespaceIgnored(repoPath, file)
        : await gitApi.getDiff(repoPath, file)
      const parsed = await diffApi.parseDiff(raw)
      repoState.update((s) => ({ ...s, activeFileDiff: parsed, isDiffLoading: false }))
    } catch (error) {
      repoState.update((s) => ({ ...s, isDiffLoading: false, error: String(error) }))
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
    repoState.update((s) => ({
      ...s,
      activeCommitFile: file,
      activeCommitFileDiff: null,
      isCommitDiffLoading: file !== null,
    }))
    if (!file) return
    const repoPath = $appState.repoPath
    const commit = get(repoState).activeCommit
    if (!repoPath || !commit) {
      repoState.update((s) => ({ ...s, isCommitDiffLoading: false }))
      return
    }
    try {
      const raw = await gitApi.getCommitDiff(repoPath, commit.sha, file.path)
      const parsed = await diffApi.parseDiff(raw)
      repoState.update((s) => ({ ...s, activeCommitFileDiff: parsed, isCommitDiffLoading: false }))
    } catch (error) {
      repoState.update((s) => ({ ...s, isCommitDiffLoading: false, error: String(error) }))
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

  async function handleSwitchBranch(branch: string) {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.switchBranch(repoPath, branch)
      showBranches = false
      await refreshStatus()
      await refreshBranches()
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
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    }
  }

  function dismissError() {
    repoState.update((s) => ({ ...s, error: undefined }))
  }

  function toggleTerminal() {
    terminalVisible = !terminalVisible
  }

  function handleKeyDown(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null
    const inField = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement
    const meta = e.ctrlKey || e.metaKey

    if (e.key === 'Escape') {
      if (showBranches || showSettings || showHelp || showMerge || showPRs) {
        e.preventDefault()
        showBranches = showSettings = showHelp = showMerge = showPRs = false
        return
      }
    }

    if (inField) return

    if (e.key === '`' && !meta) {
      e.preventDefault()
      toggleTerminal()
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
    {#if $repoState.activeTab === 'changes'}
      <div class="file-list-container">
        <FileList
          files={$repoState.status.files}
          selectedFiles={$repoState.selectedFiles}
          activeFile={$repoState.activeFile}
          onActivate={handleFileActivate}
          onToggle={handleFileToggle}
        />
      </div>
      <div class="commit-section" style="height: {commitHeight}px;">
        <div
          class="commit-resize-handle"
          onmousedown={startCommitResize}
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize commit section"
        ></div>
        <CommitMessage />
      </div>
    {:else}
      <div class="commit-list-container">
        <CommitList
          commits={$repoState.log.commits}
          selectedSha={$repoState.activeCommit?.sha || null}
          onSelect={loadCommitFiles}
          onLoadMore={loadMoreCommits}
        />
      </div>
    {/if}
  </div>

  <div
    class="sidebar-resize-handle"
    onmousedown={startSidebarResize}
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
  ></div>

  <div class="main-content">
    <Header
      onOpenBranches={() => (showBranches = true)}
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
      onOpenPRs={() => (showPRs = true)}
    />

    <div class="content-area">
      {#if $repoState.activeTab === 'changes'}
        {#if $repoState.isDiffLoading}
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
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize commit files pane"
          ></div>
          <div class="commit-diff-pane">
            {#if $repoState.isCommitDiffLoading}
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

    {#if terminalVisible && $appState.repoPath}
      <div class="terminal-pane">
        <div class="terminal-header">
          <span>Terminal — {$appState.repoPath}</span>
          <button class="close-btn" onclick={toggleTerminal} title="Close (`)">✕</button>
        </div>
        <Terminal repoPath={$appState.repoPath} />
      </div>
    {/if}
  </div>

  {#if showBranches}
    <div class="overlay-backdrop" role="presentation" onclick={() => (showBranches = false)}>
      <div class="overlay-content" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
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

  .terminal-pane {
    display: flex;
    flex-direction: column;
    height: 280px;
    border-top: 1px solid var(--border-inactive);
    background: #000000;
  }

  .terminal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 12px;
    height: 28px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-inactive);
    font-size: 11px;
    color: var(--text-muted);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px 6px;
    font-size: 12px;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
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

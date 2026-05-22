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
    repoState.update((s) => ({ ...s, activeCommit: commit, activeCommitFiles: [] }))
    if (!commit) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const files = await gitApi.getCommitFiles(repoPath, commit.sha)
      repoState.update((s) => ({ ...s, activeCommitFiles: files }))
    } catch {}
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

<div class="main-layout">
  <div class="sidebar">
    <div class="file-list-container">
      <FileList
        files={$repoState.status.files}
        selectedFiles={$repoState.selectedFiles}
        activeFile={$repoState.activeFile}
        onActivate={handleFileActivate}
        onToggle={handleFileToggle}
      />
    </div>
    <CommitMessage />
  </div>

  <div class="main-content">
    <Header
      onOpenBranches={() => (showBranches = true)}
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
      onOpenPRs={() => (showPRs = true)}
    />

    <TabBar />

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
      {:else}
        <div class="history-pane">
          <CommitList
            commits={$repoState.log.commits}
            selectedSha={$repoState.activeCommit?.sha || null}
            onSelect={loadCommitFiles}
            onLoadMore={loadMoreCommits}
          />
          <CommitDetail
            commit={$repoState.activeCommit}
            files={$repoState.activeCommitFiles}
          />
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
    grid-template-columns: 320px 1fr;
    width: 100%;
    height: 100vh;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
    border-right: 1px solid var(--border-inactive);
    overflow: hidden;
    min-height: 0;
  }

  .file-list-container {
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  .main-content {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  .content-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .history-pane {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    min-height: 0;
    overflow: hidden;
  }

  .diff-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-secondary);
    background: var(--bg-primary);
  }

  .diff-empty .muted {
    color: var(--text-muted);
    font-size: 12px;
  }

  .terminal-pane {
    display: flex;
    flex-direction: column;
    height: 280px;
    border-top: 1px solid var(--border-inactive);
    background: #0d1117;
  }

  .terminal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 12px;
    height: 28px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-inactive);
    font-size: 11px;
    color: var(--text-secondary);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 2px 6px;
    font-size: 12px;
    border-radius: 3px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--bg-primary);
  }

  .overlay-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 80px;
    z-index: 1000;
  }

  .overlay-content {
    background: transparent;
  }
</style>

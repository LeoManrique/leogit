<script lang="ts">
  import { repoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { gitApi } from '$lib/api/commands'

  interface Props {
    onOpenBranches: () => void
    onOpenSettings: () => void
    onOpenHelp: () => void
    onOpenPRs: () => void
  }

  let { onOpenBranches, onOpenSettings, onOpenHelp, onOpenPRs }: Props = $props()

  let isRefreshing = $state(false)
  let isPushing = $state(false)
  let isPulling = $state(false)

  async function handleRefresh() {
    if (isRefreshing) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    isRefreshing = true
    try {
      const status = await gitApi.getStatus(repoPath)
      repoState.update((s) => ({
        ...s,
        status: {
          ...s.status,
          branch: status.branch,
          upstream: status.upstream,
          hasUpstream: status.has_upstream,
          ahead: status.ahead,
          behind: status.behind,
          files: status.files,
        },
        error: undefined,
      }))
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isRefreshing = false
    }
  }

  async function handlePull() {
    if (isPulling) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    isPulling = true
    try {
      const remote = await gitApi.getRemote(repoPath)
      await gitApi.pull(repoPath, remote)
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isPulling = false
    }
  }

  async function handlePush() {
    if (isPushing) return
    const repoPath = $appState.repoPath
    const branch = $repoState.status.branch
    if (!repoPath || !branch) return
    isPushing = true
    try {
      const remote = await gitApi.getRemote(repoPath)
      const setUpstream = !$repoState.status.hasUpstream
      await gitApi.push(repoPath, remote, branch, setUpstream, false)
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isPushing = false
    }
  }
</script>

<header class="header">
  <div class="left">
    <button class="branch-button" onclick={onOpenBranches} title="Switch branch (B)">
      <span class="branch-icon">⎇</span>
      <span class="branch-name">{$repoState.status.branch || '…'}</span>
    </button>
    <div class="status-info">
      {#if $repoState.status.hasUpstream}
        {#if $repoState.status.ahead > 0 || $repoState.status.behind > 0}
          <span class="ahead-behind">
            {#if $repoState.status.ahead > 0}↑ {$repoState.status.ahead}{/if}
            {#if $repoState.status.behind > 0}↓ {$repoState.status.behind}{/if}
          </span>
        {:else}
          <span class="upstream-ok">up to date</span>
        {/if}
      {/if}
      {#if $repoState.status.isMerging}
        <span class="merging">MERGING</span>
      {/if}
    </div>
  </div>

  <div class="right">
    <button onclick={handlePull} disabled={isPulling} title="Pull from remote">
      {isPulling ? '⟳' : '↓'} Pull
    </button>
    <button onclick={handlePush} disabled={isPushing} title="Push to remote">
      {isPushing ? '⟳' : '↑'} Push
    </button>
    <button onclick={onOpenPRs} disabled={!$appState.ghAuthed} title="Pull requests (P)">
      PRs
    </button>
    <button onclick={handleRefresh} disabled={isRefreshing} title="Refresh (Ctrl+R)">
      {isRefreshing ? '⟳' : '↻'}
    </button>
    <button onclick={onOpenSettings} title="Settings (,)">⚙</button>
    <button onclick={onOpenHelp} title="Help (?)">?</button>
  </div>
</header>

<style>
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-secondary);
    gap: 12px;
  }

  .left {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .branch-button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: background 120ms ease;
  }

  .branch-button:hover {
    background: var(--surface-hover);
  }

  .branch-icon {
    color: var(--text-muted);
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }

  .status-info {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .ahead-behind {
    color: var(--text-muted);
  }

  .upstream-ok {
    color: var(--status-green);
  }

  .merging {
    color: var(--status-yellow);
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .right {
    display: flex;
    gap: 4px;
  }

  button {
    padding: 3px 10px;
    font-size: 12px;
    cursor: pointer;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    border-radius: 6px;
    transition: background 120ms ease, color 120ms ease;
  }

  button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

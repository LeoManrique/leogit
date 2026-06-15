<script lang="ts">
  import { onMount } from 'svelte'
  import { repoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { gitApi, ghApi } from '$lib/api/commands'
  import { ensureRepoIdentifiers, repoIdentifiers } from '$lib/stores/repoIdentifiers'
  import ContextMenu, { type ContextMenuItem } from './ContextMenu.svelte'
  import ForcePushConfirm from './ForcePushConfirm.svelte'
  import PublishRepository from './PublishRepository.svelte'
  import RepoTooltip from './RepoTooltip.svelte'

  interface Props {
    onOpenRepos: () => void
    onOpenBranches: () => void
    onOpenSettings: () => void
    onOpenHelp: () => void
  }

  let { onOpenRepos, onOpenBranches, onOpenSettings, onOpenHelp }: Props = $props()

  function basename(path: string): string {
    const parts = path.split('/').filter(Boolean)
    return parts[parts.length - 1] || path
  }

  // Fetch the GitHub repo identifier for the current path so the chip can
  // show `name` (e.g. "rustlings-exercises") instead of the on-disk folder
  // basename. Cache is module-level — repeat path changes are free.
  $effect(() => {
    const path = $appState.repoPath
    if (path) ensureRepoIdentifiers([path])
  })

  const repoIdentifier = $derived($repoIdentifiers.get($appState.repoPath) ?? null)
  const repoName = $derived.by(() => {
    const path = $appState.repoPath
    if (!path) return ''
    return repoIdentifier?.name ?? basename(path)
  })
  const repoFullLabel = $derived.by(() => {
    const path = $appState.repoPath
    if (!path) return ''
    return repoIdentifier ? `${repoIdentifier.owner}/${repoIdentifier.name}` : basename(path)
  })

  // Chip tooltip position. Anchored just below the chip on hover/focus.
  // Same dwell delay as the repo dropdown so quick scans don't flash a tooltip.
  let chipTooltip = $state<{ x: number; y: number } | null>(null)
  const CHIP_TOOLTIP_DELAY_MS = 500
  let chipTooltipTimer: ReturnType<typeof setTimeout> | null = null
  function showChipTooltip(e: Event) {
    if (!$appState.repoPath) return
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const next = { x: rect.left, y: rect.bottom + 4 }
    if (chipTooltipTimer) clearTimeout(chipTooltipTimer)
    chipTooltipTimer = setTimeout(() => {
      chipTooltip = next
      chipTooltipTimer = null
    }, CHIP_TOOLTIP_DELAY_MS)
  }
  function hideChipTooltip() {
    if (chipTooltipTimer) {
      clearTimeout(chipTooltipTimer)
      chipTooltipTimer = null
    }
    chipTooltip = null
  }

  let isRefreshing = $state(false)
  let isPushing = $state(false)
  let isPulling = $state(false)

  let pushMenu = $state<{ x: number; y: number } | null>(null)
  let showForcePushConfirm = $state(false)
  let showPublish = $state(false)
  let isPublishing = $state(false)
  // Cache the remote name so the confirm dialog can show it without re-querying.
  let cachedRemote = $state('origin')

  const ahead = $derived($repoState.status.ahead)
  const behind = $derived($repoState.status.behind)
  const hasUpstream = $derived($repoState.status.hasUpstream)
  // A loaded branch with no remote → publish instead of push. Gating on `branch`
  // avoids briefly flashing "Publish" before the first status load resolves.
  const noRemote = $derived(
    !!$appState.repoPath && !!$repoState.status.branch && !$repoState.status.hasRemote,
  )
  // Force-push is only meaningful when the branch has diverged from its upstream
  // (commits on both sides). A plain ahead-only branch fast-forwards, so offering
  // force push there is noise — hence the menu item only appears when diverged.
  const hasDiverged = $derived(hasUpstream && ahead > 0 && behind > 0)

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
          hasRemote: status.has_remote,
          unpushedShas: new Set(status.unpushed_shas ?? []),
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
      cachedRemote = remote
      const setUpstream = !$repoState.status.hasUpstream
      await gitApi.push(repoPath, remote, branch, setUpstream, false)
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isPushing = false
    }
  }

  async function handleForcePush() {
    if (isPushing) return
    const repoPath = $appState.repoPath
    const branch = $repoState.status.branch
    if (!repoPath || !branch) return
    isPushing = true
    try {
      const remote = await gitApi.getRemote(repoPath)
      cachedRemote = remote
      const setUpstream = !$repoState.status.hasUpstream
      // 5th arg = forceWithLease. We never use bare --force.
      await gitApi.push(repoPath, remote, branch, setUpstream, true)
      await handleRefresh()
      showForcePushConfirm = false
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isPushing = false
    }
  }

  // Main split-button click: publish when there's no remote yet, otherwise push.
  function handlePushButton() {
    if (noRemote) {
      showPublish = true
    } else {
      handlePush()
    }
  }

  async function handlePublish(name: string, description: string, isPrivate: boolean) {
    if (isPublishing) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    isPublishing = true
    try {
      await ghApi.publishRepo(repoPath, name, description, isPrivate)
      showPublish = false
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      isPublishing = false
    }
  }

  function openPushMenu(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    // Anchor the menu under the chevron, aligned to its right edge.
    pushMenu = { x: rect.right - 200, y: rect.bottom + 4 }
  }

  // Cache the remote name passively so the confirm dialog has it ready.
  $effect(() => {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    gitApi.getRemote(repoPath).then((r) => (cachedRemote = r)).catch(() => {})
  })

  // Ctrl/Cmd+P triggers the primary split-button action — push, or publish when
  // the branch has no remote yet — mirroring the button's own enabled state.
  // Registered globally (not gated on focus) so it works while composing a
  // commit, matching how desktop Git clients bind push.
  function handleGlobalKeyDown(e: KeyboardEvent) {
    const meta = e.ctrlKey || e.metaKey
    if (meta && (e.key === 'p' || e.key === 'P')) {
      e.preventDefault()
      if (!isPushing && !isPublishing && $appState.repoPath) handlePushButton()
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleGlobalKeyDown)
    return () => window.removeEventListener('keydown', handleGlobalKeyDown)
  })

  const pushMenuItems = $derived<ContextMenuItem[]>(
    noRemote
      ? [{ label: 'Publish to GitHub…', action: () => (showPublish = true), enabled: !isPushing }]
      : [
          {
            label: 'Push',
            action: handlePush,
            enabled: !isPushing,
          },
          // Only offered once the branch has actually diverged — see `hasDiverged`.
          ...(hasDiverged
            ? [
                {
                  label: 'Force push (with lease)…',
                  action: () => (showForcePushConfirm = true),
                  enabled: !isPushing,
                  destructive: true,
                },
              ]
            : []),
        ],
  )
</script>

<header class="header">
  <div class="left">
    <button
      class="chip-button"
      onclick={() => { hideChipTooltip(); onOpenRepos() }}
      onmouseenter={showChipTooltip}
      onmouseleave={hideChipTooltip}
      onfocus={showChipTooltip}
      onblur={hideChipTooltip}
      aria-label="Switch repository"
    >
      <svg class="chip-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M2.5 3.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z" />
      </svg>
      <span class="chip-label">{repoName || '…'}</span>
    </button>
    <button class="chip-button" onclick={onOpenBranches} title="Switch branch (Ctrl+B)">
      <svg class="chip-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="4" cy="3.5" r="1.4" />
        <circle cx="4" cy="12.5" r="1.4" />
        <circle cx="12" cy="6" r="1.4" />
        <path d="M4 5v6" />
        <path d="M4 9c0-2.5 2-3 4-3h2.6" />
      </svg>
      <span class="chip-label">{$repoState.status.branch || '…'}</span>
    </button>
    <div class="status-info">
      <!-- Ahead/behind counts live on the Pull/Push buttons, so the bar here
           only confirms the in-sync state and never duplicates those numbers. -->
      {#if hasUpstream && ahead === 0 && behind === 0}
        <span class="upstream-ok"><span class="sync-dot"></span>up to date</span>
      {/if}
      {#if $repoState.status.isMerging}
        <span class="merging">MERGING</span>
      {/if}
    </div>
  </div>

  <div class="right">
    <button
      class="count-button"
      onclick={handlePull}
      disabled={isPulling}
      title={behind > 0 ? `Pull ${behind} commit${behind === 1 ? '' : 's'} from remote` : 'Pull from remote'}
    >
      {#if isPulling}
        <svg class="icon spinning" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
          <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
          <polyline points="13.5,2 13.5,5 10.5,5" />
        </svg>
      {:else}
        <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <line x1="8" y1="3" x2="8" y2="12" />
          <polyline points="4,8 8,12 12,8" />
        </svg>
      {/if}
      <span>Pull</span>
      {#if behind > 0}<span class="count-badge">{behind}</span>{/if}
    </button>
    <div class="split-button">
      <button
        class="count-button split-main"
        onclick={handlePushButton}
        disabled={isPushing || isPublishing}
        title={noRemote
          ? 'Publish this repository to GitHub (Ctrl+P)'
          : ahead > 0
            ? `Push ${ahead} commit${ahead === 1 ? '' : 's'} to remote (Ctrl+P)`
            : 'Push to remote (Ctrl+P)'}
      >
        {#if isPushing || isPublishing}
          <svg class="icon spinning" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
            <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
            <polyline points="13.5,2 13.5,5 10.5,5" />
          </svg>
        {:else if noRemote}
          <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M4.5 11.5a2.5 2.5 0 0 1 0-5 3.2 3.2 0 0 1 6.1-1 2.4 2.4 0 0 1 .9 4.6" />
            <line x1="8" y1="7.5" x2="8" y2="13" />
            <polyline points="6,9.5 8,7.5 10,9.5" />
          </svg>
        {:else}
          <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="8" y1="4" x2="8" y2="13" />
            <polyline points="4,8 8,4 12,8" />
          </svg>
        {/if}
        <span>{noRemote ? 'Publish' : 'Push'}</span>
        {#if !noRemote && ahead > 0}<span class="count-badge">{ahead}</span>{/if}
      </button>
      <button
        class="split-chevron"
        onclick={openPushMenu}
        disabled={isPushing || isPublishing}
        aria-label="More push options"
        title="More push options"
      >
        <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="4,6 8,10 12,6" />
        </svg>
      </button>
    </div>
    <button class="icon-button" onclick={handleRefresh} disabled={isRefreshing} title="Refresh (Ctrl+R)" aria-label="Refresh">
      <svg class="icon" class:spinning={isRefreshing} width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
        <polyline points="13.5,2 13.5,5 10.5,5" />
      </svg>
    </button>
    <button class="icon-button" onclick={onOpenSettings} title="Settings (Ctrl+,)" aria-label="Settings">
      <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="8" cy="8" r="2.2" />
        <path d="M13 9.5l1.2.7-1 1.7-1.3-.5a4.6 4.6 0 0 1-1.4.8L10.2 14H7.8l-.3-1.8a4.6 4.6 0 0 1-1.4-.8l-1.3.5-1-1.7L5 9.5a4.7 4.7 0 0 1 0-3L3.8 5.8l1-1.7 1.3.5a4.6 4.6 0 0 1 1.4-.8L7.8 2h2.4l.3 1.8a4.6 4.6 0 0 1 1.4.8l1.3-.5 1 1.7-1.2.7a4.7 4.7 0 0 1 0 3z" />
      </svg>
    </button>
    <button class="icon-button" onclick={onOpenHelp} title="Help (?)" aria-label="Help">
      <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="8" cy="8" r="6" />
        <path d="M6.2 6.2a2 2 0 1 1 2.6 2.4c-.4.2-.8.5-.8 1.1" />
        <circle cx="8" cy="11.6" r="0.5" fill="currentColor" stroke="none" />
      </svg>
    </button>
  </div>
</header>

{#if chipTooltip && $appState.repoPath}
  <RepoTooltip
    title={repoFullLabel}
    path={$appState.repoPath}
    x={chipTooltip.x}
    y={chipTooltip.y}
  />
{/if}

{#if pushMenu !== null}
  <ContextMenu
    x={pushMenu.x}
    y={pushMenu.y}
    items={pushMenuItems}
    onClose={() => (pushMenu = null)}
  />
{/if}

{#if showForcePushConfirm}
  <ForcePushConfirm
    branch={$repoState.status.branch}
    remote={cachedRemote}
    {isPushing}
    onConfirm={handleForcePush}
    onCancel={() => (showForcePushConfirm = false)}
  />
{/if}

{#if showPublish}
  <PublishRepository
    defaultName={repoName}
    {isPublishing}
    onPublish={handlePublish}
    onCancel={() => (showPublish = false)}
  />
{/if}

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

  .chip-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    box-sizing: border-box;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: background 120ms ease, border-color 120ms ease;
    min-width: 0;
  }

  .chip-button:hover {
    background: var(--surface-hover);
  }

  .chip-icon {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .chip-label {
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

  .upstream-ok {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-muted);
  }

  .sync-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--status-green);
    flex-shrink: 0;
  }

  .merging {
    color: var(--status-yellow);
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .right {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    box-sizing: border-box;
    padding: 0 10px;
    font-size: 12px;
    cursor: pointer;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    border-radius: 6px;
    transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
    font-family: inherit;
  }

  button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /*
    Pull / Push share the chips' elevated treatment so the whole bar reads as
    one consistent button family rather than a mix of solid chips and ghost
    actions. The border stays static on hover (only the fill brightens) so the
    split-button seam never shifts colour mid-row.
  */
  .count-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-elevated);
    border-color: var(--border-strong);
    color: var(--text-primary);
  }

  .count-button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .icon {
    color: var(--text-muted);
    flex-shrink: 0;
    line-height: 0;
  }

  .count-button:hover:not(:disabled) .icon,
  .icon-button:hover:not(:disabled) .icon {
    color: var(--text-primary);
  }

  .icon-button {
    width: 28px;
    padding: 0;
    color: var(--text-muted);
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .count-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-secondary);
    border-radius: 999px;
    padding: 1px 6px;
    font-variant-numeric: tabular-nums;
  }

  .split-button {
    display: inline-flex;
    align-items: stretch;
    height: 28px;
    gap: 0;
  }

  .split-main {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    border-right: none;
    padding-right: 8px;
  }

  .split-chevron {
    width: 24px;
    padding: 0;
    background: var(--bg-elevated);
    border-color: var(--border-strong);
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left: 1px solid var(--border-inactive);
    color: var(--text-muted);
  }

  .split-chevron:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }
</style>

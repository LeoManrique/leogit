<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { repoState } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import {
    activeNetworkOp,
    networkProgress,
    beginNetworkOp,
    endNetworkOp,
  } from '$lib/stores/networkOps'
  import { gitApi, ghApi, type GitProgressEvent } from '$lib/api/commands'
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
  // Push/pull/publish in-flight state lives in the shared `activeNetworkOp`
  // store — not local $state — so the 2 s status poll and auto-fetch can pause
  // while a transfer runs. It also makes the ops mutually exclusive: every
  // handler guards on the store, so Pull is inert while a Push is in flight.
  const isPushing = $derived($activeNetworkOp === 'push')
  const isPulling = $derived($activeNetworkOp === 'pull')
  const isPublishing = $derived($activeNetworkOp === 'publish')
  // 0–1 fill for the in-button progress bar, GitHub-Desktop style.
  const transferFraction = $derived(($networkProgress?.percent ?? 0) / 100)

  let pushMenu = $state<{ x: number; y: number } | null>(null)
  let showForcePushConfirm = $state(false)
  let showPublish = $state(false)
  // Cache the remote name so the confirm dialog can show it without re-querying.
  let cachedRemote = $state('origin')

  const ahead = $derived($repoState.status.ahead)
  const behind = $derived($repoState.status.behind)
  const hasUpstream = $derived($repoState.status.hasUpstream)
  // Detached HEAD (after "Checkout commit"): the chip shows the short SHA instead
  // of a branch name, and push/pull are suppressed since there's no branch to
  // push. The user returns to a branch via the branch picker.
  const detached = $derived($repoState.status.detached)
  const detachedShort = $derived($repoState.status.headSha.slice(0, 7))
  // A loaded branch with no remote → publish instead of push. Gating on `branch`
  // avoids briefly flashing "Publish" before the first status load resolves.
  const noRemote = $derived(
    !!$appState.repoPath && !!$repoState.status.branch && !$repoState.status.hasRemote,
  )
  // The repo has a remote but the current branch isn't tracking anything yet →
  // "Publish branch" (push + set upstream), matching GitHub Desktop. Distinct
  // from `noRemote` (no remote at all → publish the whole repo to GitHub). Once
  // published the branch gains an upstream and this flips back to a plain Push.
  const publishBranch = $derived(
    !!$appState.repoPath &&
      !!$repoState.status.branch &&
      $repoState.status.hasRemote &&
      !hasUpstream,
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
          detached: status.detached,
          headSha: status.head_sha,
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
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    beginNetworkOp('pull')
    try {
      const remote = await gitApi.getRemote(repoPath)
      await gitApi.pull(repoPath, remote)
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      endNetworkOp()
    }
  }

  async function handlePush() {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    const branch = $repoState.status.branch
    if (!repoPath || !branch) return
    beginNetworkOp('push')
    try {
      const remote = await gitApi.getRemote(repoPath)
      cachedRemote = remote
      const setUpstream = !$repoState.status.hasUpstream
      await gitApi.push(repoPath, remote, branch, setUpstream, false)
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      endNetworkOp()
    }
  }

  async function handleForcePush() {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    const branch = $repoState.status.branch
    if (!repoPath || !branch) return
    beginNetworkOp('push')
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
      endNetworkOp()
    }
  }

  // Main split-button click: publish the repo to GitHub when there's no remote
  // yet, otherwise push. The "Publish branch" state (remote exists, branch has
  // no upstream) routes through `handlePush`, which sets the upstream because
  // `!hasUpstream` — so the same handler both publishes a branch and pushes.
  function handlePushButton() {
    if (noRemote) {
      showPublish = true
    } else {
      handlePush()
    }
  }

  async function handlePublish(name: string, description: string, isPrivate: boolean) {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    beginNetworkOp('publish')
    try {
      await ghApi.publishRepo(repoPath, name, description, isPrivate)
      showPublish = false
      await handleRefresh()
    } catch (error) {
      repoState.update((s) => ({ ...s, error: String(error) }))
    } finally {
      endNetworkOp()
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

  // A repo switch mid-transfer must not carry the old repo's progress into the
  // new repo's header — the listener already drops foreign-path events, so
  // without this the last line/fill would just freeze there until the op ends.
  $effect(() => {
    void $appState.repoPath
    networkProgress.set(null)
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
    // Live `--progress` output while a push/pull runs. Clone progress belongs
    // to the Clone dialog, and events that straggle in after the op resolved
    // (or for another repo) are dropped so an idle button never twitches.
    let unlistenProgress: (() => void) | null = null
    listen<GitProgressEvent>('git-progress', (e) => {
      const p = e.payload
      if (p.op === 'clone' || p.path !== $appState.repoPath || !$activeNetworkOp) return
      networkProgress.set({ percent: p.percent, text: p.text })
    }).then((u) => (unlistenProgress = u))
    return () => {
      window.removeEventListener('keydown', handleGlobalKeyDown)
      unlistenProgress?.()
    }
  })

  const pushMenuItems = $derived<ContextMenuItem[]>(
    noRemote
      ? [{ label: 'Publish to GitHub…', action: () => (showPublish = true), enabled: !isPushing }]
      : publishBranch
        ? [{ label: 'Publish branch', action: handlePush, enabled: !isPushing }]
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
    <button
      class="chip-button"
      onclick={onOpenBranches}
      title={detached ? 'Detached HEAD — pick a branch to return to' : 'Switch branch (Ctrl+B)'}
    >
      {#if detached}
        <!-- Commit node (detached HEAD points at a commit, not a branch). -->
        <svg class="chip-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="8" cy="8" r="2.6" />
          <path d="M2 8h2.8" />
          <path d="M11.2 8H14" />
        </svg>
      {:else}
        <svg class="chip-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="4" cy="3.5" r="1.4" />
          <circle cx="4" cy="12.5" r="1.4" />
          <circle cx="12" cy="6" r="1.4" />
          <path d="M4 5v6" />
          <path d="M4 9c0-2.5 2-3 4-3h2.6" />
        </svg>
      {/if}
      <span class="chip-label">
        {#if detached}On {detachedShort}{:else}{$repoState.status.branch || '…'}{/if}
      </span>
    </button>
    <div class="status-info">
      <!-- Ahead/behind counts live on the Pull/Push buttons, so the bar here
           only confirms the in-sync state and never duplicates those numbers. -->
      {#if detached}
        <span class="detached" title="HEAD is not on any branch">DETACHED HEAD</span>
      {/if}
      {#if $repoState.status.isMerging}
        <span class="merging">MERGING</span>
      {/if}
      {#if $networkProgress}
        <!-- Git's own progress line, verbatim — phase, counts, and throughput
             exactly as a terminal would show them. -->
        <span class="net-progress" title={$networkProgress.text}>{$networkProgress.text}</span>
      {/if}
    </div>
  </div>

  <div class="right">
    <!-- Pull only makes sense once the branch tracks a remote. Before that the
         primary button is "Publish branch" (or "Publish" with no remote), and a
         Pull button would have nothing to pull from — so hide it, matching
         GitHub Desktop, which shows only the publish affordance until then. -->
    {#if hasUpstream}
      <button
        class="count-button"
        class:in-progress={isPulling}
        onclick={handlePull}
        disabled={$activeNetworkOp !== null}
        title={behind > 0 ? `Pull ${behind} commit${behind === 1 ? '' : 's'} from remote` : 'Pull from remote'}
      >
        {#if isPulling}
          <div class="btn-progress" style:transform="scaleX({transferFraction})"></div>
        {/if}
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
        <span>{isPulling ? 'Pulling…' : 'Pull'}</span>
        {#if behind > 0 && !isPulling}<span class="count-badge">{behind}</span>{/if}
      </button>
    {/if}
    <div class="split-button">
      <button
        class="count-button split-main"
        class:in-progress={isPushing || isPublishing}
        onclick={handlePushButton}
        disabled={$activeNetworkOp !== null || detached}
        title={detached
          ? 'Detached HEAD — check out a branch to push'
          : noRemote
            ? 'Publish this repository to GitHub (Ctrl+P)'
            : publishBranch
              ? `Publish this branch to ${cachedRemote} (Ctrl+P)`
              : ahead > 0
                ? `Push ${ahead} commit${ahead === 1 ? '' : 's'} to remote (Ctrl+P)`
                : 'Push to remote (Ctrl+P)'}
      >
        {#if isPushing}
          <div class="btn-progress" style:transform="scaleX({transferFraction})"></div>
        {/if}
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
        {:else if publishBranch}
          <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="8" y1="3" x2="8" y2="10.5" />
            <polyline points="5,6 8,3 11,6" />
            <path d="M3.5 13h9" />
          </svg>
        {:else}
          <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="8" y1="4" x2="8" y2="13" />
            <polyline points="4,8 8,4 12,8" />
          </svg>
        {/if}
        <span>
          {isPushing
            ? 'Pushing…'
            : isPublishing
              ? 'Publishing…'
              : noRemote
                ? 'Publish'
                : publishBranch
                  ? 'Publish branch'
                  : 'Push'}
        </span>
        {#if !noRemote && !publishBranch && ahead > 0 && !isPushing}<span class="count-badge">{ahead}</span>{/if}
      </button>
      <button
        class="split-chevron"
        onclick={openPushMenu}
        disabled={$activeNetworkOp !== null || detached}
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
        
      <g id="lock-rotation">
        <path id="Ellipse 1115" stroke-linecap="round" d="M13.3261 8.5c-0.6772 2.8667 -3.2525 5 -6.3261 5C3.41015 13.5 0.5 10.5899 0.5 7 0.5 3.41015 3.41015 0.5 7 0.5c2.50772 0 4.6838 1.42011 5.7678 3.5" stroke-width="1"></path>
        <path id="Vector" stroke-linecap="round" stroke-linejoin="round" d="M13.5 2v2.5H11" stroke-width="1"></path>
      </g>
      </svg>
    </button>
    <button class="icon-button" onclick={onOpenSettings} title="Settings (Ctrl+,)" aria-label="Settings">
      <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path id="Vector" stroke-linecap="round" stroke-linejoin="round" d="m5.23004 2.25 0.43 -1.11c0.07252 -0.187936 0.20011 -0.349589 0.36605 -0.463788C6.19204 0.562014 6.3886 0.500595 6.59004 0.5h0.82c0.20144 0.000595 0.39801 0.062014 0.56395 0.176212 0.16595 0.114199 0.29353 0.275852 0.36605 0.463788l0.43 1.11 1.45996 0.84 1.18 -0.18c0.1965 -0.02667 0.3965 0.00567 0.5746 0.09292 0.178 0.08725 0.3261 0.22546 0.4254 0.39708l0.4 0.7c0.1025 0.17435 0.1498 0.37568 0.1355 0.57742 -0.0143 0.20174 -0.0894 0.39441 -0.2155 0.55258l-0.73 0.93v1.68l0.75 0.93c0.1261 0.15817 0.2012 0.35084 0.2155 0.55258 0.0143 0.20174 -0.033 0.40307 -0.1355 0.57742l-0.4 0.7c-0.0993 0.1716 -0.2474 0.3098 -0.4254 0.3971 -0.1781 0.0872 -0.3781 0.1196 -0.5746 0.0929l-1.18 -0.18 -1.45996 0.84 -0.43 1.11c-0.07252 0.1879 -0.2001 0.3496 -0.36605 0.4638 -0.16594 0.1142 -0.36251 0.1756 -0.56395 0.1762h-0.84c-0.20144 -0.0006 -0.398 -0.062 -0.56395 -0.1762 -0.16594 -0.1142 -0.29353 -0.2759 -0.36605 -0.4638l-0.43 -1.11 -1.46 -0.84 -1.18 0.18c-0.19648 0.0267 -0.39646 -0.0057 -0.57452 -0.0929 -0.17805 -0.0873 -0.32615 -0.2255 -0.42548 -0.3971l-0.4 -0.7c-0.1025 -0.17435 -0.14972 -0.37568 -0.13544 -0.57742 0.01428 -0.20174 0.0894 -0.39441 0.21544 -0.55258l0.73 -0.93V6.16l-0.75 -0.93c-0.12604 -0.15817 -0.20116 -0.35084 -0.21544 -0.55258 -0.01428 -0.20174 0.03294 -0.40307 0.13544 -0.57742l0.4 -0.7c0.09933 -0.17162 0.24743 -0.30983 0.42548 -0.39708 0.17806 -0.08725 0.37804 -0.11959 0.57452 -0.09292l1.18 0.18 1.48 -0.84Zm-0.23 4.75c0 0.39556 0.1173 0.78224 0.33706 1.11114 0.21977 0.3289 0.53212 0.58524 0.89757 0.73662 0.36546 0.15137 0.76759 0.19098 1.15555 0.11381 0.38796 -0.07717 0.74433 -0.26765 1.02404 -0.54736 0.2797 -0.2797 0.47018 -0.63607 0.54735 -1.02403 0.07717 -0.38796 0.03757 -0.79009 -0.11381 -1.15555 -0.15137 -0.36545 -0.40772 -0.67781 -0.73662 -0.89757C7.78228 5.1173 7.3956 5 7.00004 5c-0.53043 0 -1.03914 0.21071 -1.41421 0.58579 -0.37507 0.37507 -0.58579 0.88378 -0.58579 1.41421v0Z" stroke-width="1"></path>
      </svg>
    </button>
    <button class="icon-button" onclick={onOpenHelp} title="Help (?)" aria-label="Help">
      <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <g id="help-question-1--circle-faq-frame-help-info-mark-more-query-question">
        <path id="Vector" stroke-linecap="round" stroke-linejoin="round" d="M7 13.5c3.5899 0 6.5 -2.9101 6.5 -6.5C13.5 3.41015 10.5899 0.5 7 0.5 3.41015 0.5 0.5 3.41015 0.5 7c0 3.5899 2.91015 6.5 6.5 6.5Z" stroke-width="1"></path>
        <path id="Vector_2" stroke-linecap="round" stroke-linejoin="round" d="M5.25 5.24963c0 -0.34612 0.10273 -0.68446 0.2952 -0.97225 0.19246 -0.28778 0.46603 -0.51208 0.78609 -0.64454 0.32006 -0.13245 0.67225 -0.1671 1.01202 -0.09958 0.33978 0.06752 0.65188 0.23419 0.89685 0.47894 0.24496 0.24474 0.41178 0.55656 0.47937 0.89602 0.06758 0.33947 0.0329 0.69133 -0.09968 1.0111 -0.13257 0.31977 -0.35708 0.59308 -0.64513 0.78537 -0.28804 0.19229 -0.6267 0.29493 -0.97313 0.29493v1.16666" stroke-width="1"></path>
        <g id="Group 2567">
          <path id="Vector_3" stroke-linecap="round" stroke-linejoin="round" d="M7 10.4585c-0.13807 0 -0.25 -0.1119 -0.25 -0.25s0.11193 -0.25 0.25 -0.25" stroke-width="1"></path>
          <path id="Vector_4" stroke-linecap="round" stroke-linejoin="round" d="M7 10.4585c0.13807 0 0.25 -0.1119 0.25 -0.25s-0.11193 -0.25 -0.25 -0.25" stroke-width="1"></path>
        </g>
      </g>
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
    min-width: 0;
    overflow: hidden;
  }

  /* Git's raw progress line during a push/pull — phase, counts, throughput,
     exactly as a terminal would show them. Mono keeps the numbers from
     jittering; ellipsis keeps a long line from squeezing the chips. */
  .net-progress {
    font-family: var(--font-mono);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .merging {
    color: var(--status-yellow);
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .detached {
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
    /* Anchor + clip the in-button progress fill. */
    position: relative;
    overflow: hidden;
  }

  .count-button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  /* Content paints above the fill — an absolutely-positioned sibling would
     otherwise cover the (non-positioned) icon and label. */
  .count-button > .icon,
  .count-button > span {
    position: relative;
  }

  /* GitHub-Desktop-style progress: a full-height fill that wipes across the
     button, scaled to the aggregate transfer fraction. The transition smooths
     the jumps between git's phases. */
  .btn-progress {
    position: absolute;
    inset: 0;
    transform-origin: left;
    transform: scaleX(0);
    background: var(--surface-hover);
    transition: transform 0.3s ease-out;
    pointer-events: none;
  }

  /* Keep the label legible while the op runs — the fill + spinner already say
     "busy", so the usual disabled dimming would only gray out the progress. */
  .count-button.in-progress:disabled {
    opacity: 1;
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

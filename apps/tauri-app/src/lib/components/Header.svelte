<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { repoState, reportActionError, reportNotice } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import {
    activeNetworkOp,
    networkProgress,
    beginNetworkOp,
    endNetworkOp,
    type NetworkOpKind,
  } from '$lib/stores/networkOps'
  import {
    gitApi,
    ghApi,
    osApi,
    type GitProgressEvent,
    type SyncProposal,
  } from '$lib/api/commands'
  import { availableUpdate, updateDismissed } from '$lib/stores/update'
  import { ensureRepoIdentifiers, repoIdentifiers } from '$lib/stores/repoIdentifiers'
  import { basename } from '$lib/utils/path'
  import { isFromTerminal } from '$lib/utils/keyboard'
  import ContextMenu, { type ContextMenuItem } from './ContextMenu.svelte'
  import ForcePushConfirm from './ForcePushConfirm.svelte'
  import Icon from './Icon.svelte'
  import PublishRepository from './PublishRepository.svelte'
  import RepoTooltip from './RepoTooltip.svelte'

  interface Props {
    /** Optional: only reachable via the repo chip, which is hidden with no repo. */
    onOpenRepos?: () => void
    /** Optional: only reachable via the branch chip, hidden with no repo. */
    onOpenBranches?: () => void
    /**
     * The two chips' elements, bound out so the owner can hang each picker
     * from the control that opens it. The owner measures them at open time
     * rather than being handed a rect from the click, because ⌘B opens the
     * branch menu with no click to measure from — one anchoring path, not
     * two. Unset while the chips are hidden (no repository open): `undefined`
     * before they first mount, `null` after `bind:this` tears them down.
     */
    repoChip?: HTMLElement | null
    branchChip?: HTMLElement | null
    onOpenSettings: () => void
    onOpenHelp: () => void
    /**
     * Reload after a transfer: status **and** the log, since a pull brings in
     * commits History would otherwise show up to two seconds late.
     *
     * Owned by `MainLayout` because a status write is more than the fields
     * `get_status` returns: it also carries `is_merging`, reconciles
     * `userDeselected` against the files that still exist, drops a diff whose
     * file is gone, and feeds the picker's badge for this repo. The header used
     * to hand-roll its own write and forgot all four, which is how a stale
     * `MERGING` badge outlived an abort.
     *
     * Optional like the other repo-scoped callbacks: everything that can reach
     * it is inside the `hasRepo` block, so the pre-main header never supplies
     * one.
     */
    onTransferFinished?: () => Promise<void>
  }

  let {
    onOpenRepos,
    onOpenBranches,
    repoChip = $bindable(),
    branchChip = $bindable(),
    onOpenSettings,
    onOpenHelp,
    onTransferFinished,
  }: Props = $props()

  /**
   * Every glyph in this bar. A macOS toolbar renders a `Label`'s symbol at the
   * *large* symbol scale of the 13pt body text, not at the text's own size —
   * measured on macOS 26.6, the toolbar symbols ink at 16.5–18.5px (`folder`
   * 18.5 × 15, `arrow.triangle.branch` 14.5 × 15.5, `gearshape` 17.5,
   * `questionmark.circle` 16.5). The registry's glyphs ink at roughly twelve
   * of their sixteen grid units, so 21 puts each within about two pixels of
   * its counterpart; the two wide symbols (the folder, the sync loop) come out
   * narrower than the native's, which are wider than they are tall where ours
   * are square — a registry matter (reskin plan, P-33), not a size one.
   */
  const TOOLBAR_GLYPH = 21

  /**
   * Whether a repository is open. The header also renders in the pre-main
   * phases (picker, loading, error) so Settings and Help stay reachable when
   * there's nothing to open — everything that acts on a repo is hidden then,
   * leaving the same bar with just the app-level controls.
   *
   * Derived rather than a prop: `repoPath` is the single source of truth, and a
   * flag passed separately could disagree with it.
   */
  const hasRepo = $derived(Boolean($appState.repoPath))

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

  // Transfer state lives in the shared `activeNetworkOp` store — not local
  // $state — so the 2 s status poll and auto-fetch can pause while one runs. It
  // also makes the ops mutually exclusive: every handler guards on the store.
  const isTransferring = $derived($activeNetworkOp !== null)
  // 0–1 fill for the in-button progress bar, GitHub-Desktop style.
  const transferFraction = $derived(($networkProgress?.percent ?? 0) / 100)

  let actionMenu = $state<{ x: number; y: number } | null>(null)
  let showForcePushConfirm = $state(false)
  let showPublish = $state(false)
  // Failures raised from inside a dialog stay in it, with the fields intact:
  // a name collision or a stale lease is fixed and retried right there, and a
  // modal stacked on top would cost two dismissals to change one character.
  let publishError = $state<string | undefined>(undefined)
  let forcePushError = $state<string | undefined>(undefined)

  const ahead = $derived($repoState.status.ahead)
  const behind = $derived($repoState.status.behind)
  const hasUpstream = $derived($repoState.status.hasUpstream)
  // Detached HEAD (after "Check Out Commit…"): the chip shows the short SHA instead
  // of a branch name. The user returns to a branch via the branch picker.
  const detached = $derived($repoState.status.detached)
  const detachedShort = $derived($repoState.status.headSha.slice(0, 7))

  /**
   * The branch chip's face, in the native client's own words
   * (`BranchMenu.swift:160` `menuLabel`): the branch name, `Detached at
   * <7-char sha>` off a branch, `Detached` when even the sha is missing, and
   * `Branches` before the first status lands.
   *
   * The chip is the only place either client says which branch it is on, so
   * the two saying it differently was the difference a user would notice
   * first. `· merging` is appended in the markup rather than here because it
   * carries its own colour — see the `.merging-suffix` rule.
   */
  const branchLabel = $derived.by(() => {
    if (detached) return detachedShort ? `Detached at ${detachedShort}` : 'Detached'
    return $repoState.status.branch || 'Branches'
  })

  /**
   * The one action the repository needs next, decided by core's ladder rather
   * than re-derived here. Three loose booleans used to answer this, and could
   * disagree — which is how Push stayed enabled on a diverged branch for git to
   * reject, and how the chevron came to offer only what the face already said.
   */
  const proposal = $derived($repoState.status.proposal)

  /** The two informational states: the button names them and stays off. */
  const isActionable = $derived(proposal !== 'Loading' && proposal !== 'Detached')

  /**
   * Which states earn a chevron, as in the native client: only where the menu
   * offers something the face doesn't. Publishing a repository has no
   * secondary action, and in the Fetch state the menu's one item *is* the
   * face.
   */
  const hasMenu = $derived(
    proposal === 'PublishBranch' || proposal === 'Pull' || proposal === 'Push',
  )

  // Force-push is only meaningful once the branch has diverged from its upstream
  // (commits on both sides). A plain ahead-only branch fast-forwards, so offering
  // it there is noise — and by the ladder this can only be true in the Pull
  // state, so that is the only menu carrying the item.
  const hasDiverged = $derived(hasUpstream && ahead > 0 && behind > 0)

  const PROPOSAL_LABEL: Record<SyncProposal, string> = {
    Loading: 'Fetch',
    Detached: 'Push',
    PublishRepository: 'Publish',
    PublishBranch: 'Publish Branch',
    Pull: 'Pull',
    Push: 'Push',
    Fetch: 'Fetch',
  }

  const OP_LABEL: Record<NetworkOpKind, string> = {
    fetch: 'Fetching…',
    pull: 'Pulling…',
    push: 'Pushing…',
    publish: 'Publishing…',
  }

  const PROPOSAL_HELP: Record<SyncProposal, string> = {
    Loading: 'Loading repository status',
    Detached: 'Detached HEAD — check out a branch to push',
    PublishRepository:
      'Publish this repository to GitHub — creates the remote repo and pushes this branch (Ctrl+P)',
    PublishBranch: 'Publish this branch to the remote and start tracking it (Ctrl+P)',
    Pull: 'Pull from the remote (Ctrl+P)',
    Push: 'Push to the remote (Ctrl+P)',
    Fetch: 'Fetch from the remote — updates the counts without touching your files (Ctrl+P)',
  }

  const actionLabel = $derived(
    $activeNetworkOp ? OP_LABEL[$activeNetworkOp] : PROPOSAL_LABEL[proposal],
  )

  const actionHelp = $derived.by(() => {
    switch (proposal) {
      case 'Pull':
        return `Pull ${behind} commit${behind === 1 ? '' : 's'} from the remote (Ctrl+P)`
      case 'Push':
        return `Push ${ahead} commit${ahead === 1 ? '' : 's'} to the remote (Ctrl+P)`
      default:
        return PROPOSAL_HELP[proposal]
    }
  })

  /** Where a force push would land. Named from git's own tracking configuration
   *  rather than composed from `{remote}/{branch}`, which is wrong whenever the
   *  upstream branch has a different name — and which cost a `git remote` per
   *  repo open purely to have the string ready. */
  const forcePushTarget = $derived($repoState.status.upstream || 'the remote branch')

  async function handleFetch() {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    beginNetworkOp('fetch')
    try {
      const remote = await gitApi.getRemote(repoPath)
      if (!remote) throw new Error('This repository has no remote to fetch from.')
      // The user asked for this one and is waiting on it, so it keeps the
      // generous budget a real transfer needs; the fail-fast background budget
      // belongs to the fetches nobody is watching.
      await gitApi.fetch(repoPath, remote, false)
      await onTransferFinished?.()
    } catch (error) {
      reportActionError(error, handleFetch)
    } finally {
      endNetworkOp()
    }
  }

  async function handlePull() {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    beginNetworkOp('pull')
    try {
      const remote = await gitApi.getRemote(repoPath)
      if (!remote) throw new Error('This repository has no remote to pull from.')
      await gitApi.pull(repoPath, remote)
      await onTransferFinished?.()
    } catch (error) {
      reportActionError(error, handlePull)
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
      // Unreachable through the UI — a repo with no remote is offered Publish,
      // not Push — but saying so beats git failing on a name we invented.
      if (!remote) throw new Error('This repository has no remote to push to.')
      // Derived at click time from real tracking configuration, never
      // synthesised: this is what makes a first push `--set-upstream`, and it
      // is why Publish Branch and Push are one handler.
      const setUpstream = !$repoState.status.hasUpstream
      await gitApi.push(repoPath, remote, branch, setUpstream, false)
      await onTransferFinished?.()
    } catch (error) {
      reportActionError(error, handlePush)
    } finally {
      endNetworkOp()
    }
  }

  async function handleForcePush() {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    const branch = $repoState.status.branch
    if (!repoPath || !branch) return
    forcePushError = undefined
    beginNetworkOp('push')
    try {
      const remote = await gitApi.getRemote(repoPath)
      if (!remote) throw new Error('This repository has no remote to push to.')
      const setUpstream = !$repoState.status.hasUpstream
      // 5th arg = forceWithLease. We never use bare --force.
      await gitApi.push(repoPath, remote, branch, setUpstream, true)
      await onTransferFinished?.()
      showForcePushConfirm = false
    } catch (error) {
      // Stays open with the reason inline: a refused lease is answered by
      // fetching and pressing the same button again.
      forcePushError = String(error)
    } finally {
      endNetworkOp()
    }
  }

  /**
   * Run whatever the ladder proposes. The single entry point for the button
   * face and for Ctrl+P, so a state can never be reachable by one and not the
   * other — the shape native's `perform()` already had.
   */
  function performProposal(): void {
    if ($activeNetworkOp) return
    switch (proposal) {
      case 'Loading':
      case 'Detached':
        return
      case 'PublishRepository':
        publishError = undefined
        showPublish = true
        return
      case 'PublishBranch':
      case 'Push':
        void handlePush()
        return
      case 'Pull':
        void handlePull()
        return
      case 'Fetch':
        void handleFetch()
    }
  }

  async function handlePublish(name: string, description: string, isPrivate: boolean) {
    if ($activeNetworkOp) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    publishError = undefined
    beginNetworkOp('publish')
    try {
      await ghApi.publishRepo(repoPath, name, description, isPrivate)
      showPublish = false
      await onTransferFinished?.()
    } catch (error) {
      // The dialog keeps the typed name and description: the common failure is
      // a name already taken, and the fix is one character away.
      publishError = String(error)
    } finally {
      endNetworkOp()
    }
  }

  function openActionMenu(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    // Anchor the menu under the chevron, aligned to its right edge.
    actionMenu = { x: rect.right - 200, y: rect.bottom + 4 }
  }

  // Update chip (a newer release exists). The chip opens a small menu: copy
  // the terminal one-liner where the installer runs (macOS/Linux), or open
  // the release page (Windows, and as release notes everywhere).
  let updateMenu = $state<{ x: number; y: number } | null>(null)
  // Transient "copied" confirmation shown on the chip itself after copying.
  let updateCopied = $state(false)
  let updateCopiedTimer: ReturnType<typeof setTimeout> | null = null

  function openUpdateMenu(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    updateMenu = { x: rect.right - 200, y: rect.bottom + 4 }
  }

  async function copyInstallCommand(cmd: string) {
    try {
      await navigator.clipboard.writeText(cmd)
      updateCopied = true
      if (updateCopiedTimer) clearTimeout(updateCopiedTimer)
      updateCopiedTimer = setTimeout(() => (updateCopied = false), 2500)
    } catch (error) {
      console.error('[update] clipboard write failed:', error)
    }
  }

  // Opening the browser can fail (no `xdg-open`, a wedged handler). Surface it
  // the way every other OS hand-off in the app does rather than letting the
  // menu close with nothing happening — in the banner, since an update the user
  // was only glancing at is not worth the window.
  function openReleasePage(url: string) {
    osApi.openUrl(url).catch((error) => {
      console.error('[update] could not open the release page:', error)
      reportNotice(error)
    })
  }

  const updateMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const info = $availableUpdate
    if (!info) return []
    const cmd = info.install_command
    return [
      cmd
        ? { label: 'Copy Update Command', action: () => void copyInstallCommand(cmd) }
        : { label: 'Download from GitHub', action: () => openReleasePage(info.url) },
      // With a command the release page is still worth a link (notes, assets);
      // without one it IS the download item above, so don't repeat it.
      ...(cmd ? [{ label: 'View Release on GitHub', action: () => openReleasePage(info.url) }] : []),
      { label: 'Dismiss for This Session', action: () => updateDismissed.set(true) },
    ]
  })

  // A repo switch mid-transfer must not carry the old repo's progress into the
  // new repo's header — the listener already drops foreign-path events, so
  // without this the last line/fill would just freeze there until the op ends.
  $effect(() => {
    void $appState.repoPath
    networkProgress.set(null)
  })

  // Ctrl/Cmd+P runs whatever the ladder proposes — Fetch, Pull, Push, Publish
  // branch or Publish — so the chord and the button can never mean different
  // things, and Pull finally has a keyboard route at all (it had none while the
  // chord was hard-wired to push). Registered globally, not gated on focus, so
  // it works while composing a commit, matching how desktop Git clients bind
  // this. The one place it is *not* ours is the embedded terminal, where Ctrl+P
  // is readline's previous-history and the shell owns the key
  // (see `utils/keyboard.ts`).
  function handleGlobalKeyDown(e: KeyboardEvent) {
    if (isFromTerminal(e)) return
    const meta = e.ctrlKey || e.metaKey
    if (meta && (e.key === 'p' || e.key === 'P')) {
      e.preventDefault()
      performProposal()
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
      if (updateCopiedTimer) clearTimeout(updateCopiedTimer)
    }
  })

  /**
   * The chevron's contents, in the three states that have one. Fetch is always
   * here — it is how the user reaches the remote without a working-tree-mutating
   * pull, which this client had no route to at all — and force push joins it
   * only once the branch has actually diverged.
   *
   * The menu no longer repeats the face: a chevron whose only item was the
   * button's own action was a control that revealed nothing.
   */
  const actionMenuItems = $derived<ContextMenuItem[]>([
    { label: 'Fetch', action: handleFetch, enabled: !isTransferring },
    ...(hasDiverged
      ? [
          {
            label: 'Force Push (with Lease)…',
            action: () => {
              forcePushError = undefined
              showForcePushConfirm = true
            },
            enabled: !isTransferring,
            destructive: true,
          },
        ]
      : []),
  ])
</script>

<header class="header">
  <div class="left">
    {#if hasRepo}
    <!-- Opens the list even mid-transfer. Switching itself is what a transfer
         holds back, and the dropdown's own rows carry that — disabling the
         chip took the whole surface away with it, including Clone, which
         claims no network slot and contends with nothing a transfer is
         doing. -->
    <button
      class="chip-button"
      bind:this={repoChip}
      onclick={() => { hideChipTooltip(); onOpenRepos?.() }}
      onmouseenter={showChipTooltip}
      onmouseleave={hideChipTooltip}
      onfocus={showChipTooltip}
      onblur={hideChipTooltip}
      aria-label="Switch repository"
    >
      <Icon name="folder" size={TOOLBAR_GLYPH} class="chip-icon" />
      <span class="chip-label">{repoName || '…'}</span>
    </button>
    <button
      class="chip-button"
      bind:this={branchChip}
      onclick={onOpenBranches}
      title={detached ? 'Detached HEAD — pick a branch to return to' : 'Switch branch (Ctrl+B)'}
    >
      <!--
        One glyph in both states, as on the native chip: `BranchMenu.swift:70`
        draws `arrow.triangle.branch` whether or not HEAD is detached. The
        *label* is what reports the state; swapping the icon as well made a
        detached HEAD look like a different control rather than the same one
        saying something else.
      -->
      <Icon name="branch" size={TOOLBAR_GLYPH} class="chip-icon" />
      <!--
        An unfinished merge reads as a suffix on the branch, not as a badge
        somewhere else in the bar — `BranchMenu.swift:170` appends the same
        ` · merging` run to the same label. The one state that changes what
        half this control's items *do* belongs on the control.
      -->
      <span class="chip-label"
        >{branchLabel}{#if $repoState.status.isMerging}<span class="merging-suffix">
            · merging</span
          >{/if}</span
      >
    </button>
    <div class="status-info">
      {#if $networkProgress}
        <!-- Git's own progress line, verbatim — phase, counts, and throughput
             exactly as a terminal would show them. -->
        <span class="net-progress" title={$networkProgress.text}>{$networkProgress.text}</span>
      {/if}
    </div>
    {/if}
  </div>

  <div class="right">
    <!-- A newer release exists. Deliberately quiet — a small chip, dismissable
         for the session, that never blocks or interrupts work. -->
    {#if $availableUpdate && !$updateDismissed}
      <button
        class="update-chip"
        onclick={openUpdateMenu}
        title={updateCopied
          ? 'Command copied'
          : $availableUpdate.install_command
            ? `leogit v${$availableUpdate.version} is available — copy the update command`
            : `leogit v${$availableUpdate.version} is available — download the installer`}
      >
        <!-- The label stays put while the icon swaps to a checkmark: a text
             swap here would resize the chip and shove the whole action cluster
             sideways for the duration. -->
        {#if updateCopied}
          <Icon name="checkmark-circle" size={11} weight="semibold" />
        {:else}
          <Icon name="arrow-up-circle" size={11} />
        {/if}
        <span>Update v{$availableUpdate.version}</span>
      </button>
    {/if}
    <!-- The one adaptive sync control, replacing the separate Pull, Push and
         Refresh buttons: its face is whatever the repository needs next, and a
         forced reload is ⌘R. Three controls were what made the wrong ones
         reachable — a Push git would reject on a diverged branch, a chevron
         that only repeated its own button, and no route to a plain fetch at
         all. Everything here acts on the open repository, so it drops away in
         the pre-main phases, leaving Settings and Help. -->
    {#if hasRepo}
    <div class="split-button">
      <button
        class="count-button split-main"
        class:in-progress={isTransferring}
        class:solo={!hasMenu}
        onclick={performProposal}
        disabled={!isActionable || isTransferring}
        title={actionHelp}
      >
        {#if isTransferring}
          {#if $networkProgress}
            <div class="btn-progress" style:transform="scaleX({transferFraction})"></div>
          {:else}
            <!-- Fetch and publish report no percentages, and a push reports none
                 until git's first tick — a bar frozen at zero reads as stuck. -->
            <div class="btn-progress indeterminate"></div>
          {/if}
          <!-- The face the native sync control wears for both `.loading` and
               `.fetch` (`SyncControls.swift:154`): a two-arrow sync loop, which
               is a different statement from the one-arrow refresh the Clone
               sheet uses. -->
          <Icon name="arrow-2-circlepath" size={TOOLBAR_GLYPH} class="icon" spin />
        {:else if proposal === 'PublishRepository'}
          <Icon name="icloud-arrow-up" size={TOOLBAR_GLYPH} class="icon" />
        {:else if proposal === 'PublishBranch'}
          <Icon name="arrow-up-circle" size={TOOLBAR_GLYPH} class="icon" />
        {:else if proposal === 'Pull'}
          <Icon name="arrow-down" size={TOOLBAR_GLYPH} class="icon" />
        {:else if proposal === 'Push' || proposal === 'Detached'}
          <Icon name="arrow-up" size={TOOLBAR_GLYPH} class="icon" />
        {:else}
          <Icon name="arrow-2-circlepath" size={TOOLBAR_GLYPH} class="icon" />
        {/if}
        <span>{actionLabel}</span>
        <!-- Both pending counts ride the one button, GitHub-Desktop style: with
             a single control the proposed action's number alone would hide the
             other half of a diverged branch, and a bare figure beside a face
             that says "Pull" can't say which direction it counts. -->
        {#if !isTransferring && behind > 0}
          <span class="count-badge" role="img" aria-label="{behind} commit{behind === 1 ? '' : 's'} to pull">
            <!-- `bold` because the badge is 8px: an SF Symbol's stroke tracks
                 the text beside it, and a regular-weight glyph this small goes
                 faint next to the 10px figure it labels. -->
            <Icon name="arrow-down" size={8} weight="bold" />
            {behind}
          </span>
        {/if}
        {#if !isTransferring && ahead > 0}
          <span class="count-badge" role="img" aria-label="{ahead} commit{ahead === 1 ? '' : 's'} to push">
            <Icon name="arrow-up" size={8} weight="bold" />
            {ahead}
          </span>
        {/if}
      </button>
      {#if hasMenu}
        <button
          class="split-chevron"
          onclick={openActionMenu}
          disabled={isTransferring}
          aria-label="More sync options"
          title="More sync options"
        >
          <Icon name="chevron-down" size={9} weight="medium" />
        </button>
      {/if}
    </div>
    {/if}
    <!--
      Settings and Help are the two glyphs with no counterpart on the native
      side at all: macOS puts Settings in the app menu and lets the menu bar's
      own key equivalents stand in for a shortcut sheet, so `Sources/` contains
      neither a gear nor a question mark. A Tauri window has no app menu, so
      the controls cannot go away — they are drawn to sit in the same family as
      the rest rather than matched to a symbol that does not exist.
    -->
    <button class="icon-button" onclick={onOpenSettings} title="Settings (Ctrl+,)" aria-label="Settings">
      <Icon name="gear" size={TOOLBAR_GLYPH} class="icon" />
    </button>
    <button class="icon-button" onclick={onOpenHelp} title="Help (?)" aria-label="Help">
      <Icon name="question-circle" size={TOOLBAR_GLYPH} class="icon" />
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

{#if updateMenu !== null}
  <ContextMenu
    x={updateMenu.x}
    y={updateMenu.y}
    items={updateMenuItems}
    onClose={() => (updateMenu = null)}
  />
{/if}

{#if actionMenu !== null}
  <ContextMenu
    x={actionMenu.x}
    y={actionMenu.y}
    items={actionMenuItems}
    onClose={() => (actionMenu = null)}
  />
{/if}

{#if showForcePushConfirm}
  <ForcePushConfirm
    upstream={forcePushTarget}
    isPushing={$activeNetworkOp === 'push'}
    error={forcePushError}
    onConfirm={handleForcePush}
    onCancel={() => (showForcePushConfirm = false)}
  />
{/if}

{#if showPublish}
  <PublishRepository
    defaultName={repoName}
    isPublishing={$activeNetworkOp === 'publish'}
    error={publishError}
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

    /*
      The toolbar's controls are capsules, and only the toolbar's: macOS 26
      draws every control in a window toolbar as a capsule, and it is the
      single most recognisable cue that a bar is a *toolbar* rather than a
      strip of buttons. Nothing else in the app takes this — `STYLE.md`'s 6px
      keeps every other surface.

      Written as half the control height rather than `999px`, which `STYLE.md`
      names as an anti-pattern and which would be wrong here anyway: these
      controls have one fixed height, so an exact half is a capsule, and a
      number that says *why* it is that number survives a height change with a
      visible error instead of a silent one. Every radius in this bar reads
      this token so the chips, the sync face and its chevron cannot drift into
      three different shapes.
    */
    --toolbar-radius: 14px;
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
    border-radius: var(--toolbar-radius);
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: background 120ms ease, border-color 120ms ease;
    min-width: 0;
  }

  .chip-button:hover {
    background: var(--surface-hover);
  }

  /* `:global` because the icon is now a child component's element, and Svelte
     stamps a scope hash only on elements in *this* component's template — a
     plain `.chip-icon` rule would compile away as unused and the glyph would
     inherit the label's colour. Sizing and `flex-shrink` are gone: `Icon` owns
     both, so there is one place to change them. */
  .chip-button :global(.chip-icon) {
    color: var(--text-muted);
  }

  .chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }

  /* Takes exactly the space left over between the chips and the action
     cluster (flex-basis 0), so however long the progress line gets it can
     only truncate — it never squeezes the chips or the buttons. */
  .status-info {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* Git's raw progress line during a push/pull — phase, counts, throughput,
     exactly as a terminal would show them. Mono keeps the numbers from
     jittering; min-width lets flex shrink it so the ellipsis can kick in. */
  .net-progress {
    font-family: var(--font-mono);
    color: var(--text-muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Purple, not the yellow a warning would take, and not the red that already
     means Deleted: `Color.merging` (FileStatusStyle.swift:12) is the one hue
     that says "git couldn't merge this", and the conflicted rows in the
     Changes tab wear it too. A coloured run also survives being clipped far
     better than a grey one, which matters because the branch name truncates
     ahead of it in a narrow window. */
  .merging-suffix {
    color: var(--status-purple);
  }

  .right {
    display: flex;
    gap: 6px;
    align-items: center;
    /* The action cluster never shrinks — the progress text truncates instead
       of squeezing the buttons. */
    flex-shrink: 0;
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
    border-radius: var(--toolbar-radius);
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
    The sync control shares the chips' elevated treatment so the whole bar reads
    as one consistent button family rather than a mix of solid chips and ghost
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

  /* Content paints above the fill — `.btn-progress` is absolutely positioned,
     and a positioned box paints after non-positioned inline content, so
     without a position of their own the glyph and the label would be *under*
     the progress wipe rather than over it.

     `:global()` on the icon half because the glyph is `<Icon>`'s element, not
     this template's: Svelte stamps its scope hash only on elements it emits
     itself, so a bare `.icon` here silently matches nothing. It compiled away
     as an unused selector when the inline `<svg>` became a component, which
     would have taken the label above the fill and left the glyph beneath it. */
  .count-button > :global(.icon),
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

  /* Indeterminate: a partial fill sweeping across, for the operations git
     never reports a percentage for (fetch, publish) and for the opening
     moments of a push. The determinate bar is a scaleX of the full width, so
     this one has to be sized instead of scaled. */
  .btn-progress.indeterminate {
    right: auto;
    width: 45%;
    transform: none;
    transition: none;
    animation: sweep 1.2s ease-in-out infinite;
  }

  @keyframes sweep {
    from {
      transform: translateX(-110%);
    }
    to {
      transform: translateX(232%);
    }
  }

  /* Keep the label legible while the op runs — the fill + spinner already say
     "busy", so the usual disabled dimming would only gray out the progress. */
  .count-button.in-progress:disabled {
    opacity: 1;
  }

  /* See the note on `.chip-icon`: tint only, reached through `:global`.
     `flex-shrink` and the `line-height: 0` that was fighting the inline-svg
     baseline both live in `Icon` now — it renders `display: block`, so there
     is no descender gap left to cancel. */
  .count-button :global(.icon),
  .icon-button :global(.icon) {
    color: var(--text-muted);
  }

  .count-button:hover:not(:disabled) :global(.icon),
  .icon-button:hover:not(:disabled) :global(.icon) {
    color: var(--text-primary);
  }

  .icon-button {
    width: 28px;
    padding: 0;
    color: var(--text-muted);
  }

  /* Two of these can sit side by side on a diverged branch, so each carries its
     own direction arrow — with one adaptive button the face no longer says
     which way a bare number counts. */
  .count-badge {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-secondary);
    border-radius: 999px;
    padding: 1px 5px 1px 4px;
    font-variant-numeric: tabular-nums;
  }

  /* Update availability is informational, not an action the user owes us — so
     the chip is tinted rather than solid, sitting a step below Pull/Push in
     the bar's visual hierarchy while still reading as "new". */
  .update-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 9px;
    color: var(--status-blue);
    border-color: color-mix(in srgb, var(--status-blue) 40%, transparent);
    background: color-mix(in srgb, var(--status-blue) 12%, transparent);
    font-weight: 500;
    white-space: nowrap;
  }

  .update-chip:hover:not(:disabled) {
    color: var(--status-blue);
    background: color-mix(in srgb, var(--status-blue) 20%, transparent);
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

  /* The states with no secondary action carry no chevron, so the face is the
     whole control again and gets its right edge back. */
  .split-main.solo {
    border-top-right-radius: var(--toolbar-radius);
    border-bottom-right-radius: var(--toolbar-radius);
    border-right: 1px solid var(--border-strong);
    padding-right: 10px;
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

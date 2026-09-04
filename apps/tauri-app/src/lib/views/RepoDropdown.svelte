<script lang="ts">
  import {
    collidingRepoLabels,
    ensureRepoIdentifiers,
    repoFullLabel,
    repoIdentifiers,
    repoLabel,
    repoSearchLabels,
  } from '$lib/stores/repoIdentifiers'
  import { repoSync } from '$lib/stores/repoSync'
  import { repoSyncScheduler } from '$lib/services/repoSyncScheduler'
  import { discoveringRepos } from '$lib/services/repoDiscovery'
  import Icon from '$lib/components/Icon.svelte'
  import RepoDiscoveryFailure from '$lib/components/RepoDiscoveryFailure.svelte'
  import { recentRepos, repoSortMode } from '$lib/stores/reposState'
  import { activeNetworkOp } from '$lib/stores/networkOps'
  import RepoTooltip from '$lib/components/RepoTooltip.svelte'
  import RepoListEmptyState from '$lib/components/RepoListEmptyState.svelte'
  import RepoSortToggle from '$lib/components/RepoSortToggle.svelte'
  import { scanFolders } from '$lib/stores/config'
  import { reposApi } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import { nextActiveIndex, scrollIntoViewWhenActive } from '$lib/actions/listNavigation'

  interface Props {
    repos: string[]
    currentRepo: string
    onSelect: (repo: string) => void
    onClone: () => void
    /** Opens Settings, for the empty state's call to action — the scan paths
     *  are what discovery walks, so "found nothing" leads here. */
    onOpenSettings: () => void
    /** Dismiss without choosing. The popover registers this on the overlay
     *  stack, so Escape reaches it wherever focus happens to be. */
    onClose: () => void
  }

  let {
    repos = [],
    currentRepo = '',
    onSelect,
    onClone,
    onOpenSettings,
    onClose,
  }: Props = $props()

  let filter = $state('')

  // Where the floating tooltip should render (clientX/Y), and which repo it
  // belongs to. We anchor below the hovered row, slightly to the right of
  // the row's left edge so the tooltip doesn't cover the label.
  let hoverTooltip = $state<{ repo: string; x: number; y: number } | null>(null)
  // Show only after a brief dwell so quick scans don't flash tooltips.
  const TOOLTIP_DELAY_MS = 500
  let tooltipTimer: ReturnType<typeof setTimeout> | null = null

  // Fetch identifiers (row labels) for every repo whenever the list changes.
  // The cache is module-level, so reopening the dropdown is free after the
  // first time.
  $effect(() => {
    ensureRepoIdentifiers(repos)
    // Badges + dirty dot for rows the tiered scheduler never reaches: a
    // fetch-less local sweep, run while the list is actually on screen.
    void repoSyncScheduler.syncVisibleRepos(repos)
  })

  /*
    Row order: the open repo, then most-recently-*used* first, then a
    name-ordered tail of everything never opened in this app.

    Recency of use, not of last commit. The old sort shelled out one
    `git log -1` per repo on every open — unbounded in parallel — to answer a
    different question than a switcher is asked: "where did a commit land most
    recently" can easily be someone else's work you just fetched. It also
    reordered the list under the cursor as the timestamps streamed in, so the
    row you were aiming at moved while you clicked. The MRU is already on disk,
    already loaded, and costs nothing.
  */
  const sortedRepos = $derived.by(() => {
    const ids = $repoIdentifiers
    const mru = $recentRepos
    const byName = (a: string, b: string) =>
      repoLabel(a, ids).localeCompare(repoLabel(b, ids), undefined, {
        sensitivity: 'base',
      })
    // Anything the MRU has never seen shares the last rank, so the name order
    // below decides the tail.
    const rank = (path: string) => {
      const index = mru.indexOf(path)
      return index === -1 ? Number.MAX_SAFE_INTEGER : index
    }
    const activeFirst = (a: string, b: string) =>
      Number(b === currentRepo) - Number(a === currentRepo)
    return $repoSortMode === 'name'
      ? [...repos].sort((a, b) => activeFirst(a, b) || byName(a, b))
      : [...repos].sort((a, b) => activeFirst(a, b) || rank(a) - rank(b) || byName(a, b))
  })

  // Best match first, and only then the chosen sort order — the keyboard
  // cursor starts on the top row, so a query has to put the repo the user
  // typed there. Core's ranking is stable, so equal matches keep
  // `sortedRepos` order. Both labels are searchable, including the
  // owner-qualified one the rows actually display.
  let filteredRepos = $state<string[]>([])

  $effect(() => {
    const q = filter.trim()
    const list = sortedRepos
    const ids = $repoIdentifiers
    const folders = $scanFolders
    if (!q) {
      filteredRepos = list
      return
    }
    let cancelled = false
    reposApi
      .filterRepos(
        q,
        list.map((path) => ({ path, names: repoSearchLabels(path, ids) })),
        folders
      )
      .then((matched) => {
        if (!cancelled) filteredRepos = matched
      })
      .catch(() => {
        if (!cancelled) filteredRepos = list
      })
    return () => {
      cancelled = true
    }
  })

  // Computed over the whole list, not the filtered one, so a row's owner
  // prefix doesn't appear and disappear as the user types.
  const collidingLabels = $derived(collidingRepoLabels(sortedRepos, $repoIdentifiers))

  // There is one global network slot, so switching away mid-transfer would
  // leave the old repo's push running with nothing reporting it while the new
  // repo's polling sat gated for invisible reasons — the header would read
  // "Pushing…" over a repo that isn't pushing. The hold belongs to the rows,
  // not to the chip that opens this list: browsing and cloning contend with
  // nothing, and cloning claims no slot in either client.
  const switchBlockedReason = $derived(
    $activeNetworkOp
      ? 'Finishing the current transfer — switching repositories is unavailable'
      : undefined,
  )

  function handleSelect(repo: string) {
    if (repo === currentRepo || switchBlockedReason) return
    onSelect(repo)
  }

  // Keyboard cursor over the filtered list. A new query rebuilds the list, so
  // the highlight snaps back to the top match — Enter then targets a sensible
  // default without any arrowing.
  let activeIndex = $state(0)
  $effect(() => {
    filter
    activeIndex = 0
  })

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filteredRepos.length, 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filteredRepos.length, -1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const repo = filteredRepos[activeIndex]
      if (repo) handleSelect(repo)
    }
  }

  function showTooltip(e: Event, repo: string) {
    // A held-back row already carries its explanation in `title`; the hover
    // card would sit on top of it saying something else.
    if (switchBlockedReason) return
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    // Anchor just below the row, offset right so the tooltip doesn't sit
    // directly on top of the label.
    const next = { repo, x: rect.left + 24, y: rect.bottom + 4 }
    if (tooltipTimer) clearTimeout(tooltipTimer)
    tooltipTimer = setTimeout(() => {
      hoverTooltip = next
      tooltipTimer = null
    }, TOOLTIP_DELAY_MS)
  }

  function hideTooltip() {
    if (tooltipTimer) {
      clearTimeout(tooltipTimer)
      tooltipTimer = null
    }
    hoverTooltip = null
  }
</script>

<div class="repo-dropdown" use:dismissOnEscape={onClose}>
  <div class="filter-row">
    <input
      type="text"
      class="filter-input"
      placeholder="Filter repositories…"
      bind:value={filter}
      onkeydown={handleKeyDown}
      use:autofocus
    />
    <RepoSortToggle />
  </div>

  <RepoDiscoveryFailure />

  <div class="repo-list">
    {#if filteredRepos.length === 0}
      <RepoListEmptyState
        discovering={$discoveringRepos && repos.length === 0}
        hasRepos={repos.length > 0}
        scannedPaths={$scanFolders}
        {onOpenSettings}
      />
    {:else}
      {#each filteredRepos as repo, i (repo)}
        {@const id = $repoIdentifiers.get(repo)}
        {@const label = repoLabel(repo, $repoIdentifiers)}
        {@const prefix = collidingLabels.has(label) && id ? `${id.owner}/` : ''}
        {@const isCurrent = repo === currentRepo}
        {@const sync = $repoSync.get(repo)}
        <button
          class="repo-item"
          class:current={isCurrent}
          class:active={i === activeIndex}
          use:scrollIntoViewWhenActive={i === activeIndex}
          onclick={() => handleSelect(repo)}
          onmouseenter={(e) => showTooltip(e, repo)}
          onmouseleave={hideTooltip}
          onfocus={(e) => showTooltip(e, repo)}
          onblur={hideTooltip}
          class:blocked={switchBlockedReason !== undefined}
          aria-disabled={switchBlockedReason !== undefined}
          title={switchBlockedReason}
        >
          <span class="repo-name">
            {#if prefix}<span class="repo-owner">{prefix}</span>{/if}{label}
          </span>
          <!-- Dirty dot: uncommitted changes — shown exactly when that repo's
               Changes tab would list at least one file. -->
          {#if sync?.dirty}
            <span class="dirty-dot" title="Uncommitted changes"></span>
          {/if}
          <!-- Behind (pull / down arrow) then ahead (push / up arrow), matching
               the header's Pull/Push glyphs. Shown only when non-zero. -->
          {#if sync && sync.behind > 0}
            <span class="sync-badge behind" title={`${sync.behind} commit${sync.behind === 1 ? '' : 's'} to pull`}>
              <Icon name="arrow-down" size={11} weight="medium" />
              {sync.behind}
            </span>
          {/if}
          {#if sync && sync.ahead > 0}
            <span class="sync-badge ahead" title={`${sync.ahead} commit${sync.ahead === 1 ? '' : 's'} to push`}>
              <Icon name="arrow-up" size={11} weight="medium" />
              {sync.ahead}
            </span>
          {/if}
        </button>
      {/each}
    {/if}
  </div>

  <!--
    Getting a repository that isn't in the list. Outside the list branch on
    purpose, so it is still there in the empty and no-matches states — which is
    exactly when the user needs it, and the reason the empty state used to be a
    dead end. A repository that exists locally and isn't here is one the scan
    paths don't cover; that is Settings' job, which the empty state links to.
  -->
  <div class="footer">
    <button class="footer-btn" onclick={onClone}>Clone Repository…</button>
  </div>
</div>

{#if hoverTooltip}
  {@const id = $repoIdentifiers.get(hoverTooltip.repo)}
  <RepoTooltip
    title={repoFullLabel(hoverTooltip.repo, $repoIdentifiers)}
    path={hoverTooltip.repo}
    x={hoverTooltip.x}
    y={hoverTooltip.y}
  />
{/if}

<style>
  .repo-dropdown {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 340px;
    max-height: 420px;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .filter-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .filter-input {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 13px;
  }

  .filter-input:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  /* Icon-only filter-row button (the sort toggle); its tooltip comes from the
     button's title. */

  .repo-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }

  /* Footer action: a text button rather than the filter row's icon treatment —
     it is the one escape hatch from the list, so it reads better labelled than
     as a glyph competing with the sort toggle above it. */
  .footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 8px 10px;
    border-top: 1px solid var(--border-inactive);
  }

  .footer-btn {
    padding: 3px 8px;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-family: inherit;
    font-size: 12px;
    white-space: nowrap;
    transition:
      color 100ms ease,
      background 100ms ease;
  }

  .footer-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .repo-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    transition: background 100ms ease;
  }

  .repo-item:hover {
    background: var(--surface-hover);
  }

  .repo-item.current {
    background: var(--bg-tertiary);
  }

  /* Keyboard cursor (arrow-key highlight). A ring rather than a fill so it
     composes with the .current row's background and reads as "focused". */
  .repo-item.active {
    box-shadow: inset 0 0 0 1.5px var(--border-active);
  }

  /* Held back by a transfer. `aria-disabled` rather than the `disabled`
     attribute, deliberately: a disabled button fires no pointer events, so
     the title explaining *why* the row can't be picked would never appear —
     which is the whole reason the row is dimmed rather than hidden. */
  .repo-item.blocked {
    cursor: default;
    opacity: 0.55;
  }

  .repo-item.blocked:hover {
    background: transparent;
  }

  .repo-name {
    flex: 1;
    color: var(--text-primary);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-item.current .repo-name {
    font-weight: 500;
  }

  /*
    Disambiguation prefix (e.g. "owner/") shown only when two repos share
    the same primary label. Rendered slightly muted so the actual repo
    name still reads as the primary token.
  */
  .repo-owner {
    color: var(--text-muted);
  }

  /*
    Per-repo pull/push badges. Behind (pull) and ahead (push) reuse the header
    Pull/Push arrow glyphs so the whole app speaks one visual language. Kept
    muted and compact so a scan of the list reads as names first, counts second;
    they brighten with the row on hover.
  */
  .sync-badge {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    line-height: 1;
  }

  .repo-item:hover .sync-badge,
  .repo-item.current .sync-badge {
    color: var(--text-secondary);
  }

  /*
    Dirty dot: uncommitted changes in that repo's working tree — the picker's
    miniature of the Changes tab, shown iff the tab would list files. Same
    muted-then-brighten treatment as the sync badges so a scan still reads
    names first, indicators second.
  */
  .dirty-dot {
    flex: 0 0 auto;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  .repo-item:hover .dirty-dot,
  .repo-item.current .dirty-dot {
    background: var(--text-secondary);
  }

</style>

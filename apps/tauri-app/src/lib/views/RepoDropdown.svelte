<script lang="ts">
  import { ensureRepoIdentifiers, repoIdentifiers } from '$lib/stores/repoIdentifiers'
  import { repoSync } from '$lib/stores/repoSync'
  import { repoSyncScheduler } from '$lib/services/repoSyncScheduler'
  import { discoveringRepos } from '$lib/services/repoDiscovery'
  import { recentRepos, repoSortMode, setRepoSortMode } from '$lib/stores/reposState'
  import type { RepoIdentifier } from '$lib/api/commands'
  import RepoTooltip from '$lib/components/RepoTooltip.svelte'
  import RepoListEmptyState from '$lib/components/RepoListEmptyState.svelte'
  import { scanFolders } from '$lib/stores/config'
  import { reposApi } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { nextActiveIndex, scrollIntoViewWhenActive } from '$lib/actions/listNavigation'
  import { basename } from '$lib/utils/path'

  interface Props {
    repos: string[]
    currentRepo: string
    onSelect: (repo: string) => void
    onClone: () => void
    /** Opens Settings, for the empty state's call to action — the scan paths
     *  are what discovery walks, so "found nothing" leads here. */
    onOpenSettings: () => void
  }

  let { repos = [], currentRepo = '', onSelect, onClone, onOpenSettings }: Props = $props()

  let filter = $state('')
  // Sort mode lives in the persisted store so toggling sticks across opens and
  // restarts. Mirrors the Clone dialog's clock / A→Z button.
  function toggleSort() {
    setRepoSortMode($repoSortMode === 'recent' ? 'name' : 'recent')
  }

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

  /** GitHub repo name when known, else folder basename. The primary row label. */
  function primaryLabel(path: string, id: RepoIdentifier | null | undefined): string {
    return id?.name ?? basename(path)
  }

  /** Full identifier when known, else basename. Used by the hover tooltip. */
  function fullLabel(path: string, id: RepoIdentifier | null | undefined): string {
    return id ? `${id.owner}/${id.name}` : basename(path)
  }

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
      primaryLabel(a, ids.get(a)).localeCompare(primaryLabel(b, ids.get(b)), undefined, {
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
        list.map((path) => {
          const id = ids.get(path)
          return { path, names: [primaryLabel(path, id), fullLabel(path, id)] }
        }),
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

  // Map primary label → count, so rows that collide get an `owner/` prefix
  // (matches GH Desktop's needsDisambiguation behavior).
  const labelCounts = $derived.by(() => {
    const ids = $repoIdentifiers
    const counts = new Map<string, number>()
    for (const p of sortedRepos) {
      const label = primaryLabel(p, ids.get(p))
      counts.set(label, (counts.get(label) ?? 0) + 1)
    }
    return counts
  })

  function needsDisambiguation(label: string): boolean {
    return (labelCounts.get(label) ?? 0) > 1
  }

  function handleSelect(repo: string) {
    if (repo === currentRepo) return
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

<div class="repo-dropdown">
  <div class="filter-row">
    <input
      type="text"
      class="filter-input"
      placeholder="Filter repositories…"
      bind:value={filter}
      onkeydown={handleKeyDown}
      use:autofocus
    />
    <!-- Sort toggle — same clock / A→Z control as the Clone dialog. The glyph
         itself is the state label; recency here is when you last opened the
         repo, which is the question a switcher is actually asked. -->
    <button
      class="icon-btn"
      onclick={toggleSort}
      title={$repoSortMode === 'recent' ? 'Sorted by recently opened' : 'Sorted alphabetically'}
      aria-label={$repoSortMode === 'recent' ? 'Sorted by recently opened' : 'Sorted alphabetically'}
    >
      {#if $repoSortMode === 'recent'}
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="4.25" cy="8" r="4" />
          <path d="M4.25 5.5V8l1.5 0.9" />
          <path d="M12.5 3.5v8" />
          <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
        </svg>
      {:else}
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <text x="0.5" y="6.6" font-size="6.5" font-weight="700" fill="currentColor" font-family="-apple-system, system-ui, sans-serif">A</text>
          <text x="0.5" y="14.8" font-size="6.5" font-weight="700" fill="currentColor" font-family="-apple-system, system-ui, sans-serif">Z</text>
          <g fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12.5 3.5v8" />
            <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
          </g>
        </svg>
      {/if}
    </button>
  </div>

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
        {@const label = primaryLabel(repo, id)}
        {@const prefix = needsDisambiguation(label) && id ? `${id.owner}/` : ''}
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
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <line x1="8" y1="3" x2="8" y2="12" />
                <polyline points="4,8 8,12 12,8" />
              </svg>
              {sync.behind}
            </span>
          {/if}
          {#if sync && sync.ahead > 0}
            <span class="sync-badge ahead" title={`${sync.ahead} commit${sync.ahead === 1 ? '' : 's'} to push`}>
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <line x1="8" y1="4" x2="8" y2="13" />
                <polyline points="4,8 8,4 12,8" />
              </svg>
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
    title={fullLabel(hoverTooltip.repo, id)}
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
  .icon-btn {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    transition:
      color 100ms ease,
      background 100ms ease;
  }

  .icon-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

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

  .sync-badge svg {
    flex-shrink: 0;
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

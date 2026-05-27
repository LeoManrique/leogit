<script lang="ts">
  import { ensureRepoIdentifiers, repoIdentifiers } from '$lib/stores/repoIdentifiers'
  import type { RepoIdentifier } from '$lib/api/commands'
  import RepoTooltip from '$lib/components/RepoTooltip.svelte'

  interface Props {
    repos: string[]
    currentRepo: string
    onSelect: (repo: string) => void
  }

  let { repos = [], currentRepo = '', onSelect }: Props = $props()

  let filter = $state('')

  // Where the floating tooltip should render (clientX/Y), and which repo it
  // belongs to. We anchor below the hovered row, slightly to the right of
  // the row's left edge so the tooltip doesn't cover the label.
  let hoverTooltip = $state<{ repo: string; x: number; y: number } | null>(null)
  // Show only after a brief dwell so quick scans don't flash tooltips.
  const TOOLTIP_DELAY_MS = 500
  let tooltipTimer: ReturnType<typeof setTimeout> | null = null

  function basename(path: string): string {
    const parts = path.split('/').filter(Boolean)
    return parts[parts.length - 1] || path
  }

  function fuzzyMatch(query: string, target: string): boolean {
    const q = query.toLowerCase()
    const t = target.toLowerCase()
    let i = 0
    for (let j = 0; j < t.length && i < q.length; j++) {
      if (t[j] === q[i]) i++
    }
    return i === q.length
  }

  // Fetch identifiers for every repo in the list whenever the list changes.
  // Cache is module-level so reopening the dropdown is free.
  $effect(() => {
    ensureRepoIdentifiers(repos)
  })

  /** GitHub repo name when known, else folder basename. The primary row label. */
  function primaryLabel(path: string, id: RepoIdentifier | null | undefined): string {
    return id?.name ?? basename(path)
  }

  /** Full identifier when known, else basename. Used by the hover tooltip. */
  function fullLabel(path: string, id: RepoIdentifier | null | undefined): string {
    return id ? `${id.owner}/${id.name}` : basename(path)
  }

  const sortedRepos = $derived.by(() => {
    const ids = $repoIdentifiers
    const list = [...repos].sort((a, b) =>
      primaryLabel(a, ids.get(a)).localeCompare(
        primaryLabel(b, ids.get(b)),
        undefined,
        { sensitivity: 'base' },
      ),
    )
    if (currentRepo) {
      const idx = list.indexOf(currentRepo)
      if (idx > 0) {
        list.splice(idx, 1)
        list.unshift(currentRepo)
      }
    }
    return list
  })

  const filteredRepos = $derived.by(() => {
    const q = filter.trim()
    if (!q) return sortedRepos
    const ids = $repoIdentifiers
    return sortedRepos.filter((p) => {
      const id = ids.get(p)
      return (
        fuzzyMatch(q, primaryLabel(p, id)) ||
        fuzzyMatch(q, fullLabel(p, id)) ||
        fuzzyMatch(q, p)
      )
    })
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

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && filteredRepos.length > 0) {
      e.preventDefault()
      handleSelect(filteredRepos[0])
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
      autofocus
    />
  </div>

  <div class="repo-list">
    {#if filteredRepos.length === 0}
      <div class="empty">No repositories</div>
    {:else}
      {#each filteredRepos as repo (repo)}
        {@const id = $repoIdentifiers.get(repo)}
        {@const label = primaryLabel(repo, id)}
        {@const prefix = needsDisambiguation(label) && id ? `${id.owner}/` : ''}
        {@const isCurrent = repo === currentRepo}
        <button
          class="repo-item"
          class:current={isCurrent}
          onclick={() => handleSelect(repo)}
          onmouseenter={(e) => showTooltip(e, repo)}
          onmouseleave={hideTooltip}
          onfocus={(e) => showTooltip(e, repo)}
          onblur={hideTooltip}
        >
          <span class="repo-name">
            {#if prefix}<span class="repo-owner">{prefix}</span>{/if}{label}
          </span>
        </button>
      {/each}
    {/if}
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
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .filter-input {
    width: 100%;
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

  .repo-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }

  .empty {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-faint);
    font-size: 12px;
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

</style>

<script lang="ts">
  import type { CommitInfo } from '$lib/api/commands'
  import ContextMenu, { type ContextMenuItem } from './ContextMenu.svelte'

  interface Props {
    commits: CommitInfo[]
    selectedSha: string | null
    unpushedShas?: Set<string>
    /**
     * Whether the backend was able to resolve an upstream ref (explicit or
     * inferred) for the current branch. Used to gate "Undo last commit…" —
     * when an upstream is known, we only allow undo on unpushed commits;
     * when no upstream is resolvable (brand new repo, no remote), we can't
     * prove a commit is pushed, so we allow undo unconditionally on the top.
     */
    hasResolvedUpstream?: boolean
    onSelect: (commit: CommitInfo) => void
    onLoadMore: () => void
    onAmendCommit?: (commit: CommitInfo) => void
    onUndoCommit?: (commit: CommitInfo) => void
  }

  let {
    commits = [],
    selectedSha = null,
    unpushedShas = new Set<string>(),
    hasResolvedUpstream = false,
    onSelect,
    onLoadMore,
    onAmendCommit,
    onUndoCommit,
  }: Props = $props()

  let contextMenu = $state<{ x: number; y: number; commit: CommitInfo; idx: number } | null>(
    null,
  )

  function openContextMenu(e: MouseEvent, commit: CommitInfo, idx: number) {
    e.preventDefault()
    e.stopPropagation()
    contextMenu = { x: e.clientX, y: e.clientY, commit, idx }
  }

  const menuItems = $derived<ContextMenuItem[]>(
    contextMenu === null
      ? []
      : [
          {
            label: 'Amend last commit…',
            // Only the top commit (idx 0 AND it's still the head of the loaded log)
            // can be amended without rewriting earlier history.
            enabled: contextMenu.idx === 0 && onAmendCommit !== undefined,
            action: () => {
              if (contextMenu) onAmendCommit?.(contextMenu.commit)
            },
          },
          {
            label: 'Undo last commit…',
            // Only the top commit, and only when we believe it's still local —
            // either we can prove it's unpushed, or we couldn't resolve an
            // upstream at all (so we can't prove it's pushed either).
            enabled:
              contextMenu.idx === 0 &&
              onUndoCommit !== undefined &&
              (!hasResolvedUpstream || unpushedShas.has(contextMenu.commit.sha)),
            action: () => {
              if (contextMenu) onUndoCommit?.(contextMenu.commit)
            },
          },
        ],
  )

  const ROW_HEIGHT = 24
  const VISIBLE_ROWS = 24
  const LOAD_MORE_OFFSET = 200

  let containerHeight = ROW_HEIGHT * VISIBLE_ROWS
  let scrollTop = $state(0)
  let scrollContainer: HTMLElement

  function handleScroll(e: Event) {
    scrollTop = (e.target as HTMLElement).scrollTop
    const target = e.target as HTMLElement
    const scrollDist = target.scrollHeight - target.scrollTop - target.clientHeight
    if (scrollDist < LOAD_MORE_OFFSET) {
      onLoadMore()
    }
  }

  function getVisibleRange() {
    const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 5)
    const endIndex = Math.min(commits.length, Math.ceil((scrollTop + containerHeight) / ROW_HEIGHT) + 5)
    return { startIndex, endIndex }
  }

  // Relative timestamp for every commit, regardless of age. Matches GitHub
  // Desktop's tiering so old commits read as "5 months ago" instead of an
  // absolute date the user has to mentally diff against today.
  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    const diffMs = Date.now() - date.getTime()
    const mins = Math.floor(diffMs / 60_000)
    const hours = Math.floor(diffMs / 3_600_000)
    const days = Math.floor(diffMs / 86_400_000)
    const months = Math.floor(days / 30)
    const years = Math.floor(days / 365)

    if (mins < 1) return 'just now'
    if (mins < 60) return mins === 1 ? '1 minute ago' : `${mins} minutes ago`
    if (hours < 24) return hours === 1 ? '1 hour ago' : `${hours} hours ago`
    if (days < 30) return days === 1 ? '1 day ago' : `${days} days ago`
    if (years < 1) return months === 1 ? '1 month ago' : `${months} months ago`
    return years === 1 ? '1 year ago' : `${years} years ago`
  }

  // Full date-time for the row title attribute; the browser shows it on hover.
  // Uses the user's locale via `dateStyle: 'full', timeStyle: 'short'`.
  function formatDateFull(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString(undefined, {
      dateStyle: 'full',
      timeStyle: 'short',
    })
  }

  function handleRowKeyDown(e: KeyboardEvent, commit: CommitInfo) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onSelect(commit)
    }
  }

  $effect(() => {
    if (scrollContainer) {
      containerHeight = scrollContainer.clientHeight
    }
  })

  const { startIndex, endIndex } = $derived.by(() => getVisibleRange())
  const visibleCommits = $derived(commits.slice(startIndex, endIndex))
  const offsetPx = $derived(startIndex * ROW_HEIGHT)
</script>

<div class="commit-list" bind:this={scrollContainer} onscroll={handleScroll}>
  <div class="virtual-scroll" style="height: {commits.length * ROW_HEIGHT}px">
    <div class="visible-items" style="transform: translateY({offsetPx}px)">
      {#each visibleCommits as commit, i (commit.sha)}
        <div
          class="commit-row"
          class:selected={commit.sha === selectedSha}
          class:unpushed={unpushedShas.has(commit.sha)}
          title={formatDateFull(commit.author_date)}
          onclick={() => onSelect(commit)}
          oncontextmenu={(e) => openContextMenu(e, commit, startIndex + i)}
          onkeydown={(e) => handleRowKeyDown(e, commit)}
          role="button"
          tabindex="0"
        >
          <code class="sha-text">{commit.short_sha}</code>
          <div class="commit-summary">{commit.summary}</div>
          <div class="commit-date">{formatDate(commit.author_date)}</div>
          {#if unpushedShas.has(commit.sha)}
            <svg
              class="unpushed-icon"
              width="9"
              height="9"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-label="Not yet pushed"
            >
              <polyline points="4,7 8,3 12,7" />
              <line x1="8" y1="3" x2="8" y2="13" />
            </svg>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<!--
  ContextMenu MUST be rendered outside `.visible-items` because that wrapper
  has `transform: translateY(...)` for virtual scrolling, which establishes a
  containing block for any `position: fixed` descendant and would offset the
  menu off-screen.
-->
{#if contextMenu !== null}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    items={menuItems}
    onClose={() => (contextMenu = null)}
  />
{/if}

<style>
  .commit-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--bg-primary);
    border-right: 1px solid var(--border-inactive);
    padding: 4px 6px;
  }

  .virtual-scroll {
    position: relative;
  }

  .visible-items {
    will-change: transform;
  }

  .commit-row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    height: 24px;
    padding: 0 8px;
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    transition: background 100ms ease;
    user-select: none;
    overflow: hidden;
  }

  /* Reserve a trailing slot for the unpushed indicator only on rows that
     actually have one — keeps in-sync rows flush with the right edge. */
  .commit-row.unpushed {
    grid-template-columns: auto minmax(0, 1fr) auto 12px;
  }

  .unpushed-icon {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .commit-row:hover {
    background: var(--surface-hover);
  }

  .commit-row.selected {
    background: var(--bg-tertiary);
  }

  .sha-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    background: transparent;
    font-variant-numeric: tabular-nums;
  }

  .commit-summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-size: 13px;
    min-width: 0;
  }

.commit-date {
    color: var(--text-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    text-align: right;
  }
</style>

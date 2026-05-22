<script lang="ts">
  import type { CommitInfo } from '$lib/api/commands'

  interface Props {
    commits: CommitInfo[]
    selectedSha: string | null
    onSelect: (commit: CommitInfo) => void
    onLoadMore: () => void
  }

  let { commits = [], selectedSha = null, onSelect, onLoadMore }: Props = $props()

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

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    const now = new Date()
    const diffMs = now.getTime() - date.getTime()
    const diffMins = Math.floor(diffMs / 60000)
    const diffHours = Math.floor(diffMs / 3600000)
    const diffDays = Math.floor(diffMs / 86400000)

    if (diffMins < 1) return 'just now'
    if (diffMins < 60) return `${diffMins}m ago`
    if (diffHours < 24) return `${diffHours}h ago`
    if (diffDays < 7) return `${diffDays}d ago`

    return date.toLocaleDateString()
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
      {#each visibleCommits as commit (commit.sha)}
        <div
          class="commit-row"
          class:selected={commit.sha === selectedSha}
          onclick={() => onSelect(commit)}
          onkeydown={(e) => handleRowKeyDown(e, commit)}
          role="button"
          tabindex="0"
        >
          <code class="sha-text">{commit.short_sha}</code>
          <div class="commit-summary">
            {#each commit.refs as ref}<span class="ref-tag">{ref}</span>{/each}{commit.summary}
          </div>
          <div class="commit-date">{formatDate(commit.author_date)}</div>
        </div>
      {/each}
    </div>
  </div>
</div>

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

  .ref-tag {
    color: var(--status-green);
    font-size: 12px;
    font-weight: 500;
    margin-right: 6px;
  }

  .commit-date {
    color: var(--text-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    text-align: right;
  }
</style>

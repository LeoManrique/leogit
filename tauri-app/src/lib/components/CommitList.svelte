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
    /**
     * Absolute index (0 = HEAD) of `commits[0]` in the full repo log. The
     * parent slides this window forward/backward as the user scrolls past
     * the cap. When it changes between renders, we compensate `scrollTop`
     * by the same pixel amount so the visible row stays pinned across the
     * slide (otherwise the user's cursor would jump up/down by N rows).
     */
    windowStartOffset?: number
    /**
     * Whether the initial history load has completed. Gates the "No commits
     * yet" empty state so it only shows for a genuinely empty repo (e.g. a
     * freshly initialized one), not during the brief window before the first
     * `get_log` resolves on a repo that does have commits.
     */
    loaded?: boolean
    onSelect: (commit: CommitInfo) => void
    onLoadMore: () => void
    onLoadEarlier?: () => void
    onAmendCommit?: (commit: CommitInfo) => void
    onUndoCommit?: (commit: CommitInfo) => void
  }

  let {
    commits = [],
    selectedSha = null,
    unpushedShas = new Set<string>(),
    hasResolvedUpstream = false,
    windowStartOffset = 0,
    loaded = true,
    onSelect,
    onLoadMore,
    onLoadEarlier,
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
          { separator: true, label: '', action: () => {} },
          {
            label: 'Copy SHA',
            action: () => {
              if (contextMenu) copySha(contextMenu.commit)
            },
          },
          {
            label: 'Copy Tag',
            // Only meaningful when the commit actually carries a tag.
            enabled: contextMenu.commit.refs.some((ref) => ref.startsWith('tag: ')),
            action: () => {
              if (contextMenu) copyTag(contextMenu.commit)
            },
          },
        ],
  )

  // Two-line rows (summary + indicators / author + date), so taller than the
  // old single-line list. Must stay in sync with `.commit-row { height }`.
  const ROW_HEIGHT = 50
  const VISIBLE_ROWS = 14
  const LOAD_MORE_OFFSET = 200

  let containerHeight = $state(ROW_HEIGHT * VISIBLE_ROWS)
  let scrollTop = $state(0)
  let scrollContainer = $state<HTMLElement>()

  function handleScroll(e: Event) {
    const target = e.target as HTMLElement
    scrollTop = target.scrollTop
    const scrollDist = target.scrollHeight - target.scrollTop - target.clientHeight
    if (scrollDist < LOAD_MORE_OFFSET) {
      onLoadMore()
    }
    // Slide-backward: trigger when the user scrolls toward the top AND we
    // know there's earlier history out of the window (windowStartOffset>0).
    // Without the offset guard we'd spam onLoadEarlier on every fresh log
    // load where scrollTop is naturally 0.
    if (target.scrollTop < LOAD_MORE_OFFSET && windowStartOffset > 0) {
      onLoadEarlier?.()
    }
  }

  function getVisibleRange() {
    const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 5)
    const endIndex = Math.min(commits.length, Math.ceil((scrollTop + containerHeight) / ROW_HEIGHT) + 5)
    return { startIndex, endIndex }
  }

  // A ticking "now" so the relative dates stay live while History is open:
  // formatDate() reads this, so bumping it re-renders the visible "N minutes
  // ago" labels. The 10 s cadence stays effectively free because the work is
  // gated on visibility — we skip the tick entirely when the History pane is
  // hidden (Changes tab → clientHeight 0) or the window is backgrounded — and
  // the list is virtualized, so an on-screen tick only re-renders the ~14
  // mounted rows. The interval is torn down with the component.
  let now = $state(Date.now())
  $effect(() => {
    const id = setInterval(() => {
      if (document.hidden || containerHeight === 0) return
      now = Date.now()
    }, 10_000)
    return () => clearInterval(id)
  })

  // Relative timestamp for every commit, regardless of age. Matches GitHub
  // Desktop's tiering so old commits read as "5 months ago" instead of an
  // absolute date the user has to mentally diff against today.
  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    const diffMs = now - date.getTime()
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

  // `refs` carries every symbolic ref pointing at the commit (branches, HEAD,
  // tags) as git's %D emits them — e.g. "tag: v0.1.0". Keep only tags and strip
  // the "tag: " prefix so we can render them as GitHub-Desktop-style pills.
  function tagsFor(commit: CommitInfo): string[] {
    return commit.refs
      .filter((ref) => ref.startsWith('tag: '))
      .map((ref) => ref.slice(5))
  }

  async function copySha(commit: CommitInfo) {
    try {
      await navigator.clipboard.writeText(commit.sha)
    } catch {}
  }

  async function copyTag(commit: CommitInfo) {
    const tags = tagsFor(commit)
    if (tags.length === 0) return
    try {
      // Space-separate when a commit carries more than one tag.
      await navigator.clipboard.writeText(tags.join(' '))
    } catch {}
  }

  // Track the viewport height with a ResizeObserver, not a one-shot read. The
  // History pane is display:none while the Changes tab is active, so measuring
  // clientHeight once at mount can capture 0 and strand the virtual window at
  // ~5 rows (ceil(0 / ROW_HEIGHT) + buffer). The observer fires when the pane
  // gains size, keeping containerHeight (and thus the rendered range) correct.
  $effect(() => {
    const el = scrollContainer
    if (!el) return
    const ro = new ResizeObserver(() => {
      const h = el.clientHeight
      // Pane just became visible again (Changes → History): refresh the dates
      // immediately instead of waiting up to 10 s for the next gated tick.
      if (h > 0 && containerHeight === 0) now = Date.now()
      containerHeight = h
    })
    ro.observe(el)
    containerHeight = el.clientHeight
    return () => ro.disconnect()
  })

  const { startIndex, endIndex } = $derived.by(() => getVisibleRange())
  const visibleCommits = $derived(commits.slice(startIndex, endIndex))
  const offsetPx = $derived(startIndex * ROW_HEIGHT)

  /*
    Compensate scrollTop when the parent slides the window. Forward slide
    (drop from front, append to back) shifts every remaining commit `N` rows
    upward in the new array — we subtract `N * ROW_HEIGHT` from scrollTop so
    the user's visible row stays anchored. Slide-backward is symmetric.

    Reading `windowStartOffset` inside the effect makes it the dependency;
    we hold the previous value in $state so the diff is observable across
    runs without infinite-looping.
  */
  let lastWindowStart = $state<number | null>(null)
  $effect(() => {
    const offset = windowStartOffset
    if (lastWindowStart === null) {
      lastWindowStart = offset
      return
    }
    const delta = offset - lastWindowStart
    if (delta === 0 || !scrollContainer) return
    scrollContainer.scrollTop -= delta * ROW_HEIGHT
    scrollTop = scrollContainer.scrollTop
    lastWindowStart = offset
  })
</script>

<div class="commit-list" bind:this={scrollContainer} onscroll={handleScroll}>
  {#if loaded && commits.length === 0}
    <div class="empty-state">
      <p>No commits yet</p>
    </div>
  {/if}
  <div class="virtual-scroll" style="height: {commits.length * ROW_HEIGHT}px">
    <div class="visible-items" style="transform: translateY({offsetPx}px)">
      {#each visibleCommits as commit, i (commit.sha)}
        {@const tags = tagsFor(commit)}
        {@const isUnpushed = unpushedShas.has(commit.sha)}
        <div
          class="commit-row"
          class:selected={commit.sha === selectedSha}
          title={formatDateFull(commit.author_date)}
          onclick={() => onSelect(commit)}
          oncontextmenu={(e) => openContextMenu(e, commit, startIndex + i)}
          onkeydown={(e) => handleRowKeyDown(e, commit)}
          role="button"
          tabindex="0"
        >
          <div class="commit-line summary-line">
            <span class="commit-summary">{commit.summary}</span>
            {#if tags.length > 0 || isUnpushed}
              <div class="commit-indicators">
                {#if tags.length > 0}
                  <span class="tag-indicator" title={tags.join(', ')}>
                    <span class="tag-name">{tags[0]}</span>
                    {#if tags.length > 1}
                      <span class="tag-indicator-more">+{tags.length - 1}</span>
                    {/if}
                  </span>
                {/if}
                {#if isUnpushed}
                  <span class="unpushed-badge" title="Not yet pushed" aria-label="Not yet pushed">
                    <svg
                      class="unpushed-icon"
                      width="10"
                      height="10"
                      viewBox="0 0 16 16"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <polyline points="4,7 8,3 12,7" />
                      <line x1="8" y1="3" x2="8" y2="13" />
                    </svg>
                  </span>
                {/if}
              </div>
            {/if}
          </div>
          <div class="commit-line meta-line">
            <span class="commit-author">{commit.author_name}</span>
            <span class="commit-meta-sep">·</span>
            <span class="commit-date">{formatDate(commit.author_date)}</span>
          </div>
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

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-faint);
    font-size: 13px;
  }

  .virtual-scroll {
    position: relative;
  }

  .visible-items {
    will-change: transform;
  }

  .commit-row {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    /* Must equal ROW_HEIGHT in the script — virtualization positions rows by it. */
    height: 50px;
    padding: 0 10px;
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

  .commit-line {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* ── Line 1: summary + tag / push indicators ── */
  .summary-line {
    gap: 8px;
  }

  .commit-summary {
    /* Grow to fill the row so indicators are pushed to the right edge, and
       shrink with ellipsis when the message is long. */
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-size: 13px;
    min-width: 0;
  }

  .commit-indicators {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 50%;
  }

  .tag-indicator {
    flex: 0 1 auto;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    height: 16px;
    line-height: 16px;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }

  .tag-indicator-more {
    flex: 0 0 auto;
    height: 16px;
    line-height: 16px;
    padding: 0 5px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-size: 10px;
  }

  /*
    Unpushed-commit badge. Shares the tag badge's pill family — same 16px
    height, 5px radius, neutral --badge-bg/--badge-fg — so the two indicators
    read as one consistent set (mirrors the inspo app, where the unpushed
    indicator reuses the tag badge background). The up-arrow inside marks the
    commit as "not yet pushed", far more visible than the old bare faint icon.
  */
  .unpushed-badge {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 16px;
    min-width: 16px;
    padding: 0 4px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
  }

  .unpushed-icon {
    flex-shrink: 0;
  }

  /* ── Line 2: author · relative date ── */
  .meta-line {
    gap: 5px;
    color: var(--text-muted);
    font-size: 11.5px;
  }

  .commit-author {
    flex: 0 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .commit-meta-sep {
    flex: 0 0 auto;
  }

  .commit-date {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>

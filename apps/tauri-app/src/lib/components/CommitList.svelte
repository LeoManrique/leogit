<script lang="ts">
  import { tick } from 'svelte'
  import type { CommitInfo } from '$lib/api/commands'
  import ContextMenu, { type ContextMenuItem } from './ContextMenu.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    commits: CommitInfo[]
    selectedSha: string | null
    unpushedShas?: Set<string>
    /**
     * Whether the backend was able to resolve an upstream ref (explicit or
     * inferred) for the current branch. Used to gate "Undo Last Commit" —
     * when an upstream is known, we only allow undo on unpushed commits;
     * when no upstream is resolvable (brand new repo, no remote), we can't
     * prove a commit is pushed, so we allow undo unconditionally on the top.
     */
    hasResolvedUpstream?: boolean
    /**
     * SHA of the repository's real HEAD, from `get_status`. Amend, undo and
     * checkout are gated on it — never on a row's index, which is an index
     * into the *loaded window* and stops meaning "HEAD" the moment that window
     * slides. Undo runs `git reset --mixed HEAD~1` against the real HEAD, so a
     * row-index gate offers it on the wrong commit and seeds the composer with
     * that commit's message (FRONTEND.md §6.10 forbids the index form).
     */
    headSha?: string
    /**
     * Bumped by the parent when it re-reads the list from HEAD (HEAD moved, a
     * different repo, the first load) instead of appending a page. Row 0 is
     * then a commit the user has not seen, and their scroll offset was
     * measured against a list whose top has changed — so we go to the top.
     */
    resetSeq?: number
    /**
     * Whether the initial history load has completed. Gates the "No commits
     * yet" empty state so it only shows for a genuinely empty repo (e.g. a
     * freshly initialized one), not during the brief window before the first
     * `get_log` resolves on a repo that does have commits.
     */
    loaded?: boolean
    onSelect: (commit: CommitInfo) => void
    onLoadMore: () => void
    onAmendCommit?: (commit: CommitInfo) => void
    onUndoCommit?: (commit: CommitInfo) => void
    onCheckoutCommit?: (commit: CommitInfo) => void
  }

  let {
    commits = [],
    selectedSha = null,
    unpushedShas = new Set<string>(),
    hasResolvedUpstream = false,
    headSha = '',
    resetSeq = 0,
    loaded = true,
    onSelect,
    onLoadMore,
    onAmendCommit,
    onUndoCommit,
    onCheckoutCommit,
  }: Props = $props()

  let contextMenu = $state<{ x: number; y: number; commit: CommitInfo } | null>(null)

  /**
   * Right-click selects the row it opens on, so the menu and the detail pane
   * below can never describe two different commits. Native gets this from
   * `contextMenu(forSelectionType:)` and this client's own file list already
   * did it by hand; only here did the menu act on a commit the pane wasn't
   * showing.
   */
  function openContextMenu(e: MouseEvent, commit: CommitInfo) {
    e.preventDefault()
    e.stopPropagation()
    if (commit.sha !== selectedSha) onSelect(commit)
    contextMenu = { x: e.clientX, y: e.clientY, commit }
  }

  /**
   * Whether the right-clicked commit is the repository's actual HEAD. `headSha`
   * comes from `get_status`, so this stays true only for the commit the
   * rewriting actions would really act on — no matter where the loaded window
   * currently sits. An empty `headSha` (unborn branch, status not loaded yet)
   * matches nothing, which disables the rewriting actions rather than
   * mis-enabling them.
   */
  const isHeadCommit = $derived(
    contextMenu !== null && headSha !== '' && contextMenu.commit.sha === headSha,
  )

  /** A commit we can prove is *not* HEAD — the checkout target. Unknown HEAD
   *  disables it too: detaching onto the commit you are already on is a
   *  surprise, not a no-op. */
  const isPastCommit = $derived(
    contextMenu !== null && headSha !== '' && contextMenu.commit.sha !== headSha,
  )

  const menuItems = $derived<ContextMenuItem[]>(
    contextMenu === null
      ? []
      : [
          {
            label: 'Amend Last Commit…',
            // Only HEAD can be amended without rewriting earlier history.
            enabled: isHeadCommit && onAmendCommit !== undefined,
            action: () => {
              if (contextMenu) onAmendCommit?.(contextMenu.commit)
            },
          },
          {
            // No ellipsis: undo runs immediately, and nothing is lost that the
            // composer and the working tree don't now hold. The ellipsis its
            // neighbours carry is a promise of a dialog, and this one never
            // had a dialog to open.
            label: 'Undo Last Commit',
            // HEAD only, and only when we believe it's still local — either we
            // can prove it's unpushed, or we couldn't resolve an upstream at
            // all (so we can't prove it's pushed either).
            enabled:
              isHeadCommit &&
              onUndoCommit !== undefined &&
              (!hasResolvedUpstream || unpushedShas.has(contextMenu.commit.sha)),
            action: () => {
              if (contextMenu) onUndoCommit?.(contextMenu.commit)
            },
          },
          {
            // The other half of the same rule: this one *does* confirm first,
            // so it says so.
            label: 'Check Out Commit…',
            // Any commit except the current HEAD — checking out HEAD is a
            // no-op. Lands the user in a detached HEAD.
            enabled: isPastCommit && onCheckoutCommit !== undefined,
            action: () => {
              if (contextMenu) onCheckoutCommit?.(contextMenu.commit)
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
            enabled: contextMenu.commit.tags.length > 0,
            action: () => {
              if (contextMenu) copyTag(contextMenu.commit)
            },
          },
        ],
  )

  // Built from the native row rather than chosen, the same way `FileList`
  // derives its 30. `CommitRow` is a `VStack(spacing: 2)`
  // (`HistorySidebar.swift:194`) over two lines — the summary line, whose
  // height is the taller of its 13pt text (16) and its 16pt tag chips
  // (`:220`, `:247`), and the `.caption` line beneath, which macOS draws at
  // 10pt and the engine gives a 12px line box — under `.padding(.vertical, 3)`
  // (`:231`). That is 3 + 16 + 2 + 12 + 3 = 36pt of row content. The `List`
  // holding it is `.listStyle(.inset)` (`:108`), which adds 4pt above and
  // below every row on macOS, and its `intercellSpacing` height is 0 on
  // Big Sur and later, so rows abut and the pitch *is* the row height:
  // 36 + 8 = 44.
  //
  // The one point of slack in that sum is the caption's line box: AppKit gives
  // a 10pt caption 13, and the engine's `normal` gives it 12, because `normal`
  // rounds SF's ascent and descent separately and loses a point below 12pt. The
  // only way to close it is a pinned `line-height` on a single-line label,
  // which is exactly what STYLE.md's leading rule forbids — so the row keeps
  // the engine's box and centres the pair inside it, which spends the point as
  // half a pixel of air at each end.
  //
  // The number is the whole of the row's proportion, and 6pt too many turns a
  // sidebar of commits into a table of records, so it is derived and not tuned.
  // Must stay in sync with `.commit-row { height }`, which the virtualizer
  // positions by.
  const ROW_HEIGHT = 44
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
  }

  /**
   * Move keyboard focus and the selection to another row, scrolling it into
   * view first so a virtualized row that isn't mounted yet exists by the time
   * we reach for it. The file list's `focusRowAt`, which is where the pattern
   * and its `tick()` ordering are explained — this was the one list in the app
   * an arrow key did nothing in, so a keyboard user had to tab through every
   * commit to reach the next one.
   */
  async function focusRowAt(index: number) {
    const clamped = Math.max(0, Math.min(commits.length - 1, index))
    const next = commits[clamped]
    if (!next) return
    onSelect(next)

    if (scrollContainer) {
      const top = clamped * ROW_HEIGHT
      const bottom = top + ROW_HEIGHT
      const vh = scrollContainer.clientHeight
      let newScroll = scrollContainer.scrollTop
      if (top < newScroll) newScroll = top
      else if (bottom > newScroll + vh) newScroll = bottom - vh
      if (newScroll !== scrollContainer.scrollTop) {
        scrollContainer.scrollTop = newScroll
        // Synchronously, so the derived visible range updates before tick().
        scrollTop = newScroll
      }
    }

    await tick()
    scrollContainer
      ?.querySelector<HTMLDivElement>(`[data-commit-row-index="${clamped}"]`)
      ?.focus({ preventScroll: true })
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

  // Relative timestamp for every commit, regardless of age. Tiered so old
  // commits read as "5 months ago" instead of an absolute date the user has to
  // mentally diff against today.
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

  // Absolute local time for the row tooltip, which is where the exact date
  // lives now that the row itself only states an age. Abbreviated month and a
  // minute-precision time — `CommitDate.absolute`'s
  // `.formatted(date: .abbreviated, time: .shortened)`, the same string the
  // native row hands `.help()` (`HistorySidebar.swift:232`) and the same one
  // the detail card prints. `dateStyle: 'full'` spelled the weekday and month
  // out in words, which is a different sentence from the one the reference
  // shows and from the one two panes away.
  function formatDateAbsolute(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    })
  }

  function handleRowKeyDown(e: KeyboardEvent, commit: CommitInfo, index: number) {
    // Home/End first — on macOS those arrive as Cmd+ArrowUp / Cmd+ArrowDown,
    // so they have to beat the plain Arrow branches below.
    if (e.key === 'Home' || (e.key === 'ArrowUp' && e.metaKey)) {
      e.preventDefault()
      focusRowAt(0)
    } else if (e.key === 'End' || (e.key === 'ArrowDown' && e.metaKey)) {
      e.preventDefault()
      focusRowAt(commits.length - 1)
    } else if (e.key === 'ArrowDown') {
      // The container would scroll otherwise; move the selection instead.
      e.preventDefault()
      focusRowAt(index + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      focusRowAt(index - 1)
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onSelect(commit)
    }
  }

  async function copySha(commit: CommitInfo) {
    try {
      await navigator.clipboard.writeText(commit.sha)
    } catch {}
  }

  // `commit.tags` comes pre-split from the backend's %D parsing, so the rows
  // render tag pills straight off the payload.
  async function copyTag(commit: CommitInfo) {
    if (commit.tags.length === 0) return
    try {
      // Space-separate when a commit carries more than one tag.
      await navigator.clipboard.writeText(commit.tags.join(' '))
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
    Go to the top when the parent re-reads the list from HEAD.

    Paging needs no counterpart: rows are only ever appended past the ones on
    screen, so every visible row keeps its position and the viewport is already
    where the user left it. A re-read is the only move that changes what row 0
    *is* — and row 0 is then the commit they just made, checked out or undid,
    which is what they should be looking at.

    Reading `resetSeq` inside the effect makes it the dependency; the previous
    value lives in $state so the change is observable across runs without
    looping.
  */
  let lastResetSeq = $state<number | null>(null)
  $effect(() => {
    const seq = resetSeq
    if (lastResetSeq === seq) return
    const first = lastResetSeq === null
    lastResetSeq = seq
    if (first || !scrollContainer) return
    scrollContainer.scrollTop = 0
    scrollTop = 0
  })
</script>

<div
  class="commit-list"
  style="--row-height: {ROW_HEIGHT}px"
  bind:this={scrollContainer}
  onscroll={handleScroll}
>
  {#if loaded && commits.length === 0}
    <div class="empty-state">
      <p>No commits yet</p>
    </div>
  {/if}
  <div class="virtual-scroll" style="height: {commits.length * ROW_HEIGHT}px">
    <div class="visible-items" style="transform: translateY({offsetPx}px)">
      {#each visibleCommits as commit, i (commit.sha)}
        {@const rowIndex = startIndex + i}
        {@const tags = commit.tags}
        {@const isUnpushed = unpushedShas.has(commit.sha)}
        <div
          class="commit-row"
          class:selected={commit.sha === selectedSha}
          class:striped={rowIndex % 2 === 1}
          data-commit-row-index={rowIndex}
          title={formatDateAbsolute(commit.author_date)}
          onclick={() => onSelect(commit)}
          oncontextmenu={(e) => openContextMenu(e, commit)}
          onkeydown={(e) => handleRowKeyDown(e, commit, rowIndex)}
          role="button"
          tabindex="0"
        >
          <div class="summary-line">
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
                    <!-- `bold` on purpose, not for emphasis: the native draws
                         this same marker at `.system(size: 9, weight: .bold)`
                         (`HistorySidebar.swift:217`), and a symbol's stroke
                         tracks the weight of the text it sits with. -->
                    <Icon name="arrow-up" size={10} weight="bold" />
                  </span>
                {/if}
              </div>
            {/if}
          </div>
          <!--
            One text run, not three spans in a flex row. The native row's second
            line is a single interpolated `Text` — `"\(authorName) · \(relative)"`
            (`HistorySidebar.swift:226`, and the byte is U+00B7 with one ordinary
            space either side) — so the separator is worth about 2.5px of space
            at this size, where a flex `gap` would put its own value there twice
            and visibly widen the line.

            It also settles what gives way when the sidebar narrows: one run
            under `.lineLimit(1)` (`:229`) truncates at the tail, so the date is
            what goes, not the author.
          -->
          <div class="meta-line">{commit.author_name} · {formatDate(commit.author_date)}</div>
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
    /* `VStack(alignment: .leading, spacing: 2)` (`HistorySidebar.swift:194`).
       The two lines sit 2px apart and the row's remaining slack — the native
       row's own 3pt of vertical padding plus the inset `List`'s 4pt above and
       below — falls either side of the pair, which is what centring does. */
    gap: 2px;
    /* Published by the wrapper from `ROW_HEIGHT`, which is also the step the
       virtualizer positions by — one number, so they cannot drift. */
    height: var(--row-height);
    padding: 0 10px;
    /* Apple's own sample for imitating list selection draws it as
       `.rect(cornerRadius: 6)`, and `NSTableView.Style.inset` is documented to
       round the row background and its selection together. */
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    transition: background 100ms ease;
    user-select: none;
    overflow: hidden;
  }

  /*
    Alternating row backgrounds, as the native History list has
    (`HistorySidebar.swift:109` — the same `.alternatingRowBackgrounds()` the
    changed-file list calls, and the reason both Tauri lists stripe or neither
    does). Row 0 is the plain one.

    Keyed on the commit's index in the model, never on DOM position: this list
    is virtualized and its rows are translated as a block, so `:nth-child` sees
    only the slice near the viewport and would restripe the whole list on every
    scroll. `FileList.svelte` carries the same rule for the same reason, and
    the alpha is deliberately about half `--surface-hover` — AppKit's own ~4.7 %
    would land close enough to the hover fill to cost a state these lists have
    and the native ones do not.

    Declared before the three state fills: all four weigh (0,2,0), so source
    order is the whole of the cascade here and a stripe written later would
    paint over the row the pointer is on.
  */
  .commit-row.striped {
    background: var(--surface-stripe);
  }

  .commit-row:hover {
    background: var(--surface-hover);
  }

  .commit-row.selected {
    background: var(--bg-tertiary);
  }

  /* ── Line 1: summary + tag / push indicators ──
     6px between every item on this line, including between the summary and the
     first chip: the native row builds it as one `HStack(spacing: 6)` whose
     chips and unpushed plate are all direct children
     (`HistorySidebar.swift:195`), so a single spacing governs the whole run. */
  .summary-line {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: 6px;
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

  /* The tag and its `+N` companion are separate children of the native row's
     one `HStack(spacing: 6)`, so they sit 6px apart like everything else on
     the line — this wrapper only exists to keep them together when the
     indicator cluster shrinks. */
  .tag-indicator {
    flex: 0 1 auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  /*
    Both chips come out of one native builder — `chip(_:)` at
    `HistorySidebar.swift:242-250`: 10.5px mono, 5px of horizontal padding,
    a pinned 16px height and a 5px radius on the `.quaternary` plate that
    `--badge-bg` / `--badge-fg` stand for. `+N` is that same builder called
    with a different string (`:207`), so it takes the same type and the same
    padding rather than a smaller register of its own.

    `line-height: 16px` here is the pinned-box exception STYLE.md allows: it is
    the chip's geometry, matching `.frame(height: 16)`, not reading leading.
  */
  .tag-name,
  .tag-indicator-more {
    height: 16px;
    line-height: 16px;
    padding: 0 5px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-indicator-more {
    flex: 0 0 auto;
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
    /* A fixed square, not a padded minimum: the native plate is
       `.frame(width: 16, height: 16)` (`HistorySidebar.swift:220`), so it is
       exactly as wide as it is tall whatever the glyph inside measures. These
       two indicators are meant to read as one family, and a plate wider than
       the chip beside it is tall is the one proportion that breaks that. */
    width: 16px;
    height: 16px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
  }

  /* ── Line 2: author · relative date ──
     `.font(.caption).foregroundStyle(.secondary)` on the native run
     (`HistorySidebar.swift:227-228`): macOS draws `.caption` at 10pt regular,
     and `.secondary` is `--text-secondary`, one step brighter than the
     `--text-muted` that stands for `.tertiary`. Tabular digits come from the
     app-wide `body` rule, so a ticking "N minutes ago" still can't wobble. */
  .meta-line {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-size: 10px;
  }
</style>

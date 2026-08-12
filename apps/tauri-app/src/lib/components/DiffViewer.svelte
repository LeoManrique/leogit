<script lang="ts">
  import type {
    ParsedDiff,
    FileDiff,
    DiffSelection,
    DiffLine,
    SbsPair,
    BlobSource,
  } from '$lib/api/commands'
  import { highlightApi } from '$lib/api/commands'

  interface Props {
    diff: ParsedDiff | null
    selection: DiffSelection | null
    /** Lets the highlighter read each side's full blob so it can parse from
     *  line 1. Without it, highlighting falls back to a diff-only parse that is
     *  only correct when the first hunk starts in the file's top-level context. */
    blobSource?: BlobSource | null
    showSelection?: boolean
    syntaxHighlighting?: boolean
    sideBySide?: boolean
    /** When true, long lines wrap to fit the viewer; the diff body is rendered
     *  WITHOUT virtualization (variable row heights break the offset math).
     *  When false, lines stay one-line tall and the body horizontal-scrolls,
     *  keeping the cheap fixed-height virtualization that makes 10K-line
     *  diffs cost the same as 30-line ones. */
    wrapLongLines?: boolean
    tabSize?: number
    onLineToggle?: (lineIndex: number) => void
    onHunkToggle?: (hunkIndex: number) => void
  }

  let {
    diff = null,
    selection = null,
    blobSource = null,
    showSelection = false,
    syntaxHighlighting = true,
    sideBySide = false,
    wrapLongLines = true,
    tabSize = 4,
    onLineToggle = () => {},
    onHunkToggle = () => {},
  }: Props = $props()

  // The lean diff structure — what the template's line lookups and the
  // highlight round trip use. The render artifacts (html, sbs_pairs, counts)
  // live on `diff` itself.
  const fileDiff = $derived(diff?.file_diff ?? null)

  // Flat view of the diff's lines in global-index order — the indexing shared
  // by `highlightedHtml`, the selection map, and `diff.sbs_pairs`.
  const flatLines = $derived.by((): DiffLine[] => {
    const out: DiffLine[] = []
    if (fileDiff) {
      for (const h of fileDiff.hunks) for (const l of h.lines) out.push(l)
    }
    return out
  })

  /*
    `highlightedHtml[i]` is the pre-escaped HTML for the i-th flattened diff
    line, ready to drop into `{@html}`. Both phases come from Rust
    (`render.rs`), so they can never drift apart:
      Phase 1 (sync): `diff.html` ships in the parse_diff payload — plain
                      escaped text + intra-line backplate, painted the same
                      frame the diff mounts.
      Phase 2 (debounced async): `highlight_diff` returns the same lines with
                      syntect `.syn-*` spans laid over the same backplate.
    Theme swap is pure CSS (no `theme` reactive read), so toggling light/dark
    doesn't re-fetch or re-render.
  */
  let highlightedHtml = $state<string[]>([])

  const HIGHLIGHT_DEBOUNCE_MS = 80
  let highlightReq = 0
  // Re-run guard: the parent's `repoState` writable fires every 2 s on status
  // poll, which propagates through the store chain and re-evaluates
  // `diff={$repoState.activeFileDiff}` in the template even when the
  // reference is unchanged. Without this guard, every poll would re-tokenize
  // and produce a visible "highlighting flash" on the static viewer.
  let lastDiff: ParsedDiff | null = null
  let lastSyntaxHighlighting: boolean | null = null

  async function runHighlight(fd: FileDiff, source: BlobSource | null, reqId: number) {
    if (reqId !== highlightReq) return
    try {
      const html = await highlightApi.highlightDiff(fd, source)
      if (reqId !== highlightReq) return
      highlightedHtml = html
    } catch (e) {
      console.error('[DiffViewer] highlight_diff failed', e)
    }
  }

  $effect(() => {
    const pd = diff
    const sh = syntaxHighlighting
    if (pd === lastDiff && sh === lastSyntaxHighlighting) return
    lastDiff = pd
    lastSyntaxHighlighting = sh
    const myReq = ++highlightReq
    if (!pd) {
      highlightedHtml = []
      return
    }
    highlightedHtml = pd.html
    if (!sh) return
    // Read `blobSource` only past the guard: the parent builds it inline, so a
    // fresh object every status poll would otherwise retrigger this effect.
    const src = blobSource
    const t = setTimeout(() => runHighlight(pd.file_diff, src, myReq), HIGHLIGHT_DEBOUNCE_MS)
    return () => clearTimeout(t)
  })

  function isLineSelected(globalIdx: number): boolean {
    if (!selection) return false
    if (Object.prototype.hasOwnProperty.call(selection.diverging_lines, globalIdx)) {
      return selection.diverging_lines[globalIdx]
    }
    return selection.default_selected
  }

  function lineTypeClass(t: DiffLine['line_type']): string {
    switch (t) {
      case 'Add': return 'diff-add'
      case 'Delete': return 'diff-remove'
      case 'Context': return 'diff-context'
      case 'Hunk': return 'diff-hunk'
      case 'NoNewline': return 'diff-context'
      default: return ''
    }
  }

  function linePrefix(line: DiffLine): string {
    if (line.line_type === 'Add') return '+'
    if (line.line_type === 'Delete') return '-'
    if (line.line_type === 'NoNewline') return '\\'
    if (line.line_type === 'Hunk') return '@'
    return ' '
  }

  /*
    Virtualization. The diff body keeps only the visible window in the DOM
    (plus an OVERSCAN buffer on either side). A 5K-line diff previously
    mounted 30K+ spans; with virtualization, only ~30 .diff-line nodes are
    live regardless of the diff's total size.

    Uniform row heights make this cheap: `white-space: pre` on the line body
    (no wrap) gives every diff line ROW_HEIGHT, every hunk header
    HEADER_HEIGHT. Cumulative offsets are recomputed once per fileDiff
    change and binary-searched to find the visible slice from scrollTop.
  */
  const ROW_HEIGHT = 18
  const HEADER_HEIGHT = 24
  const OVERSCAN = 8

  let scrollContainer = $state<HTMLDivElement | null>(null)
  let scrollTop = $state(0)
  let containerHeight = $state(0)

  type DiffRow = {
    kind: 'header' | 'line'
    hunkIdx: number
    lineIdx: number
    globalIdx: number
    line: DiffLine
    height: number
    key: string
  }

  const rows = $derived.by((): DiffRow[] => {
    if (!fileDiff || sideBySide || fileDiff.is_binary) return []
    const out: DiffRow[] = []
    let g = 0
    for (let h = 0; h < fileDiff.hunks.length; h++) {
      const hunk = fileDiff.hunks[h]
      for (let i = 0; i < hunk.lines.length; i++) {
        const line = hunk.lines[i]
        const kind: 'header' | 'line' = line.line_type === 'Hunk' ? 'header' : 'line'
        out.push({
          kind,
          hunkIdx: h,
          lineIdx: i,
          globalIdx: g,
          line,
          height: kind === 'header' ? HEADER_HEIGHT : ROW_HEIGHT,
          key: kind === 'header' ? `H-${g}` : `L-${g}`,
        })
        g++
      }
    }
    return out
  })

  const rowOffsets = $derived.by(() => {
    const offsets = new Array<number>(rows.length + 1)
    offsets[0] = 0
    for (let i = 0; i < rows.length; i++) offsets[i + 1] = offsets[i] + rows[i].height
    return offsets
  })
  const totalHeight = $derived(rowOffsets[rowOffsets.length - 1] ?? 0)

  type SbsRow = { pair: SbsPair; height: number; key: string }

  const sbsRows = $derived.by((): SbsRow[] => {
    if (!sideBySide || !diff) return []
    return diff.sbs_pairs.map((p, i) => ({
      pair: p,
      height: p.is_hunk_header ? HEADER_HEIGHT : ROW_HEIGHT,
      key: `S-${i}`,
    }))
  })

  const sbsOffsets = $derived.by(() => {
    const offsets = new Array<number>(sbsRows.length + 1)
    offsets[0] = 0
    for (let i = 0; i < sbsRows.length; i++) offsets[i + 1] = offsets[i] + sbsRows[i].height
    return offsets
  })
  const sbsTotal = $derived(sbsOffsets[sbsOffsets.length - 1] ?? 0)

  function findIndexAt(offsets: number[], y: number, count: number): number {
    if (count === 0) return 0
    let lo = 0
    let hi = count - 1
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (offsets[mid + 1] <= y) lo = mid + 1
      else hi = mid
    }
    return lo
  }

  const startIndex = $derived(Math.max(0, findIndexAt(rowOffsets, scrollTop, rows.length) - OVERSCAN))
  const endIndex = $derived(
    Math.min(rows.length, findIndexAt(rowOffsets, scrollTop + containerHeight, rows.length) + OVERSCAN + 1),
  )
  const visibleRows = $derived(rows.slice(startIndex, endIndex))
  const offsetPx = $derived(rowOffsets[startIndex] ?? 0)

  const sbsStartIndex = $derived(
    Math.max(0, findIndexAt(sbsOffsets, scrollTop, sbsRows.length) - OVERSCAN),
  )
  const sbsEndIndex = $derived(
    Math.min(sbsRows.length, findIndexAt(sbsOffsets, scrollTop + containerHeight, sbsRows.length) + OVERSCAN + 1),
  )
  const visibleSbsRows = $derived(sbsRows.slice(sbsStartIndex, sbsEndIndex))
  const sbsOffsetPx = $derived(sbsOffsets[sbsStartIndex] ?? 0)

  // Keep containerHeight in sync with the scroll container's actual size (handles
  // pane resizes and tab visibility toggles). Mirrors CommitList's pattern.
  $effect(() => {
    const el = scrollContainer
    if (!el) return
    const ro = new ResizeObserver(() => {
      containerHeight = el.clientHeight
    })
    ro.observe(el)
    containerHeight = el.clientHeight
    return () => ro.disconnect()
  })

  // Reset scroll position when the user opens a different file or toggles the
  // layout — leaving scrollTop pinned to the previous diff's offset would land
  // them mid-file in the new diff (often past its end).
  let lastDiffKey = $state<string | null>(null)
  let lastSideBySide = $state<boolean | null>(null)
  let lastWrap = $state<boolean | null>(null)
  $effect(() => {
    const key = fileDiff ? `${fileDiff.old_path}|${fileDiff.new_path}` : null
    const sbs = sideBySide
    const wrap = wrapLongLines
    if (key !== lastDiffKey || sbs !== lastSideBySide || wrap !== lastWrap) {
      lastDiffKey = key
      lastSideBySide = sbs
      lastWrap = wrap
      scrollTop = 0
      if (scrollContainer) scrollContainer.scrollTop = 0
    }
  })
</script>

{#if fileDiff}
  <div class="diff-viewer" style="--tab-size: {tabSize}">
    <div class="file-header">
      {#if fileDiff.old_path && fileDiff.old_path !== fileDiff.new_path}
        <span class="old-path">{fileDiff.old_path}</span>
        <span class="arrow">→</span>
      {/if}
      <span class="new-path">{fileDiff.new_path || fileDiff.old_path}</span>
      {#if diff && (diff.additions > 0 || diff.deletions > 0)}
        <span class="line-counts">
          {#if diff.additions > 0}<span class="add-count">+{diff.additions}</span>{/if}
          {#if diff.deletions > 0}<span class="del-count">-{diff.deletions}</span>{/if}
        </span>
      {/if}
    </div>

    {#if fileDiff.is_binary}
      <div class="binary-state">
        <p>This binary file has changed.</p>
      </div>
    {:else if sideBySide}
      {@const renderedSbsRows = wrapLongLines ? sbsRows : visibleSbsRows}
      <div
        class="diff-body"
        class:wrap={wrapLongLines}
        bind:this={scrollContainer}
        onscroll={(e) => (scrollTop = (e.currentTarget as HTMLDivElement).scrollTop)}
      >
        <div class="diff-virtual" style:height={wrapLongLines ? 'auto' : `${sbsTotal}px`}>
          <div class="diff-visible" style:transform={wrapLongLines ? 'none' : `translateY(${sbsOffsetPx}px)`}>
            {#each renderedSbsRows as row (row.key)}
              {@const left = row.pair.left !== null ? flatLines[row.pair.left] : null}
              {@const right = row.pair.right !== null ? flatLines[row.pair.right] : null}
              {#if row.pair.is_hunk_header && left}
                <div class="hunk-header sbs-hunk-header" style:height={wrapLongLines ? null : `${HEADER_HEIGHT}px`}>
                  <span class="hunk-text">{left.text}</span>
                </div>
              {:else}
                <div class="sbs-row" style:height={wrapLongLines ? null : `${ROW_HEIGHT}px`}>
                  <div class="sbs-side sbs-left {left ? lineTypeClass(left.line_type) : 'sbs-empty'}">
                    <span class="line-number">{left?.old_line_no ?? ''}</span>
                    <span class="line-prefix">{left ? linePrefix(left) : ' '}</span>
                    <span class="line-content">
                      {#if left && row.pair.left !== null && highlightedHtml[row.pair.left]}
                        {@html highlightedHtml[row.pair.left]}
                      {:else if left}
                        {left.content}
                      {/if}
                    </span>
                  </div>
                  <div class="sbs-side sbs-right {right ? lineTypeClass(right.line_type) : 'sbs-empty'}">
                    <span class="line-number">{right?.new_line_no ?? ''}</span>
                    <span class="line-prefix">{right ? linePrefix(right) : ' '}</span>
                    <span class="line-content">
                      {#if right && row.pair.right !== null && highlightedHtml[row.pair.right]}
                        {@html highlightedHtml[row.pair.right]}
                      {:else if right}
                        {right.content}
                      {/if}
                    </span>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        </div>
      </div>
    {:else}
      {@const renderedRows = wrapLongLines ? rows : visibleRows}
      <div
        class="diff-body"
        class:diff-body-scroll={!wrapLongLines}
        class:wrap={wrapLongLines}
        bind:this={scrollContainer}
        onscroll={(e) => (scrollTop = (e.currentTarget as HTMLDivElement).scrollTop)}
      >
        <div class="diff-virtual" style:height={wrapLongLines ? 'auto' : `${totalHeight}px`}>
          <div class="diff-visible" style:transform={wrapLongLines ? 'none' : `translateY(${offsetPx}px)`}>
            {#each renderedRows as row (row.key)}
              {#if row.kind === 'header'}
                <div
                  class="hunk-header"
                  style:height={wrapLongLines ? null : `${HEADER_HEIGHT}px`}
                  onclick={(e) => { if (e.shiftKey) onHunkToggle(row.hunkIdx) }}
                  onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.shiftKey) { e.preventDefault(); onHunkToggle(row.hunkIdx) } }}
                  role="button"
                  tabindex="0"
                >
                  <span class="hunk-text">{row.line.text}</span>
                  {#if showSelection}<span class="hunk-hint">Shift+click for hunk</span>{/if}
                </div>
              {:else}
                <div class="diff-line {lineTypeClass(row.line.line_type)}" style:height={wrapLongLines ? null : `${ROW_HEIGHT}px`}>
                  <span class="line-number old">{row.line.old_line_no ?? ''}</span>
                  <span class="line-number new">{row.line.new_line_no ?? ''}</span>
                  <span class="line-prefix">{linePrefix(row.line)}</span>
                  <span class="line-content">
                    {#if highlightedHtml[row.globalIdx]}
                      {@html highlightedHtml[row.globalIdx]}
                    {:else}
                      {row.line.content}
                    {/if}
                  </span>
                  {#if showSelection && (row.line.line_type === 'Add' || row.line.line_type === 'Delete')}
                    <button
                      class="selection-dot"
                      class:selected={isLineSelected(row.globalIdx)}
                      onclick={() => onLineToggle(row.globalIdx)}
                      title="Toggle line selection"
                      aria-label="Toggle line selection"
                    ></button>
                  {/if}
                </div>
              {/if}
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
{:else}
  <div class="empty-state">
    <p>No diff to display</p>
  </div>
{/if}

<style>
  .diff-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    tab-size: var(--tab-size, 4);
    -moz-tab-size: var(--tab-size, 4);
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-faint);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  }

  /*
    Shown in place of hunk rows when the diff is for a binary file. Git can't
    produce a line-by-line diff for binaries, so we render a labelled stand-in
    while still keeping the file-header above so the user can see the path.
  */
  .binary-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-faint);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
  }

  .file-header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-inactive);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .old-path { color: var(--text-muted); }
  .arrow { color: var(--text-muted); }
  .new-path { color: var(--text-primary); }

  /* Per-file +adds/-dels totals, pushed to the right edge of the header. */
  .line-counts {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .add-count { color: var(--diff-add-fg); }
  .del-count { color: var(--diff-remove-fg); }

  /*
    .diff-body is the scroll container for the virtualized list. The unified
    layout adds .diff-body-scroll to allow horizontal scroll for long lines
    (forced no-wrap below); side-by-side keeps overflow-x: hidden so the two
    columns stay aligned and clip long lines instead of scrolling.

    .diff-virtual sets the full content height so the scrollbar is sized
    correctly even though only the visible window is mounted. .diff-visible
    is translated to the offset of the first visible row.
  */
  .diff-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
  }

  .diff-body-scroll {
    overflow-x: auto;
  }

  .diff-virtual {
    position: relative;
    width: 100%;
  }

  .diff-visible {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    will-change: transform;
  }

  .diff-body-scroll .diff-visible :global(.diff-line),
  .diff-body-scroll .diff-visible :global(.hunk-header) {
    width: max-content;
    min-width: 100%;
  }

  .hunk-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    box-sizing: border-box;
    padding: 0 12px;
    background: var(--bg-secondary);
    color: var(--text-muted);
    border-top: 1px solid var(--border-inactive);
    border-bottom: 1px solid var(--border-inactive);
    cursor: pointer;
    user-select: none;
  }

  .hunk-text {
    color: var(--text-muted);
  }

  .hunk-hint {
    font-size: 10px;
    color: var(--text-faint);
  }

  .diff-line {
    display: flex;
    align-items: center;
    box-sizing: border-box;
  }

  .diff-line.diff-add,
  .sbs-side.diff-add {
    background: var(--diff-add-bg);
  }

  .diff-line.diff-remove,
  .sbs-side.diff-remove {
    background: var(--diff-remove-bg);
  }

  .diff-line.diff-context,
  .sbs-side.diff-context {
    color: var(--text-secondary);
  }

  .line-number {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    width: 3em;
    padding: 0 6px;
    color: var(--text-muted);
    font-size: 11px;
    user-select: none;
    flex-shrink: 0;
    border-right: 1px solid var(--border-inactive);
  }

  .line-prefix {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5em;
    padding: 0 4px;
    flex-shrink: 0;
    font-weight: 500;
  }

  .diff-add .line-prefix,
  .sbs-side.diff-add .line-prefix { color: var(--diff-add-fg); }

  .diff-remove .line-prefix,
  .sbs-side.diff-remove .line-prefix { color: var(--diff-remove-fg); }

  .line-content {
    flex: 1;
    min-width: 0;
    padding: 0 8px;
    white-space: pre;
  }

  /*
    Intra-line backplates. The span is emitted inside `.line-content` via
    `{@html}`, so we have to reach it with `:global(...)` from the scoped style
    block. A tiny inset keeps the inline syntax colour visible underneath the
    backplate.
  */
  .line-content :global(.diff-intra-add) {
    background: var(--diff-intra-add-bg);
    border-radius: 2px;
  }

  .line-content :global(.diff-intra-remove) {
    background: var(--diff-intra-remove-bg);
    border-radius: 2px;
  }

  .selection-dot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 12px;
    height: 12px;
    padding: 0;
    margin: 3px 8px;
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 100ms, border-color 100ms;
  }

  .selection-dot:hover { border-color: var(--border-active); }
  .selection-dot.selected {
    background: var(--border-active);
    border-color: var(--border-active);
  }

  /* Side-by-side */
  .sbs-hunk-header {
    grid-column: 1 / -1;
  }

  .sbs-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    box-sizing: border-box;
  }

  .sbs-side {
    display: flex;
    align-items: center;
    overflow: hidden;
    border-right: 1px solid var(--border-inactive);
  }

  .sbs-side.sbs-right {
    border-right: none;
  }

  .sbs-side.sbs-empty {
    background: var(--surface-hover);
  }

  /*
    Wrap mode. `.diff-body.wrap` flips `.line-content` from `pre` to `pre-wrap`
    and forces the gutter (line numbers + prefix) to stay top-aligned with the
    first wrapped line instead of centring against the now-taller row. With
    wrap on, virtualization is disabled (variable row heights break the
    fixed-height offset math) and the .diff-virtual / .diff-visible wrappers
    are inert (height: auto, transform: none from the template).

    `overflow-wrap: anywhere` breaks even unbroken token runs (long URLs,
    minified blobs), which matches the user expectation of "no horizontal
    scroll". `word-break: break-word` would leave English-ish strings intact
    but allow breaks at any character — `anywhere` is more aggressive and
    safer for code.
  */
  .diff-body.wrap :global(.line-content) {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .diff-body.wrap :global(.diff-line),
  .diff-body.wrap :global(.sbs-side) {
    align-items: flex-start;
  }
  /* Drop the min-width rule that forces unified rows to be at least as wide
     as the scroll container — with wrap on, we want each row to live within
     the container's intrinsic width. */
  .diff-body.wrap :global(.diff-line),
  .diff-body.wrap :global(.hunk-header) {
    width: auto;
    min-width: 0;
  }
  /* Side-by-side: the columns clip with overflow: hidden in non-wrap mode.
     With wrap on, let them grow vertically so the right side's wrapped
     text is visible. */
  .diff-body.wrap :global(.sbs-side) {
    overflow: visible;
  }
</style>

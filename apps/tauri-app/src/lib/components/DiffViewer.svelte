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

  // Rows render whole, wrapped to the viewer width. Deliberately no
  // virtualization: wrapped rows have variable heights, which break
  // fixed-height offset math.

  let scrollContainer = $state<HTMLDivElement | null>(null)

  type DiffRow = {
    kind: 'header' | 'line'
    hunkIdx: number
    lineIdx: number
    globalIdx: number
    line: DiffLine
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
          key: kind === 'header' ? `H-${g}` : `L-${g}`,
        })
        g++
      }
    }
    return out
  })

  type SbsRow = { pair: SbsPair; key: string }

  const sbsRows = $derived.by((): SbsRow[] => {
    if (!sideBySide || !diff) return []
    return diff.sbs_pairs.map((p, i) => ({ pair: p, key: `S-${i}` }))
  })

  // Reset scroll position when the user opens a different file or toggles the
  // layout — leaving scrollTop pinned to the previous diff's offset would land
  // them mid-file in the new diff (often past its end).
  let lastDiffKey = $state<string | null>(null)
  let lastSideBySide = $state<boolean | null>(null)
  $effect(() => {
    const key = fileDiff ? `${fileDiff.old_path}|${fileDiff.new_path}` : null
    const sbs = sideBySide
    if (key !== lastDiffKey || sbs !== lastSideBySide) {
      lastDiffKey = key
      lastSideBySide = sbs
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
      <div class="diff-body" bind:this={scrollContainer}>
        {#each sbsRows as row (row.key)}
          {@const left = row.pair.left !== null ? flatLines[row.pair.left] : null}
          {@const right = row.pair.right !== null ? flatLines[row.pair.right] : null}
          {#if row.pair.is_hunk_header && left}
            <div class="hunk-header sbs-hunk-header">
              <span class="hunk-text">{left.text}</span>
            </div>
          {:else}
            <div class="sbs-row">
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
    {:else}
      <div class="diff-body" bind:this={scrollContainer}>
        {#each rows as row (row.key)}
          {#if row.kind === 'header'}
            <div
              class="hunk-header"
              onclick={(e) => { if (e.shiftKey) onHunkToggle(row.hunkIdx) }}
              onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.shiftKey) { e.preventDefault(); onHunkToggle(row.hunkIdx) } }}
              role="button"
              tabindex="0"
            >
              <span class="hunk-text">{row.line.text}</span>
              {#if showSelection}<span class="hunk-hint">Shift+click for hunk</span>{/if}
            </div>
          {:else}
            <div class="diff-line {lineTypeClass(row.line.line_type)}">
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
    .diff-body is the scroll container. Vertical only: long lines wrap
    (`pre-wrap` on .line-content below), so a wide line never needs a
    horizontal scrollbar and rows have variable heights.
  */
  .diff-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
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

  /* The gutter (line numbers + prefix) top-aligns with the first wrapped
     line instead of centring against a row a long line has made taller. */
  .diff-line {
    display: flex;
    align-items: flex-start;
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

  /*
    `overflow-wrap: anywhere` breaks even unbroken token runs (long URLs,
    minified blobs), which matches the user expectation of "no horizontal
    scroll". `word-break: break-word` would leave English-ish strings intact
    but allow breaks at any character — `anywhere` is more aggressive and
    safer for code.
  */
  .line-content {
    flex: 1;
    min-width: 0;
    padding: 0 8px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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

  /* Columns grow vertically with wrapped text (no clipping), gutters
     top-aligned like the unified rows. */
  .sbs-side {
    display: flex;
    align-items: flex-start;
    border-right: 1px solid var(--border-inactive);
  }

  .sbs-side.sbs-right {
    border-right: none;
  }

  .sbs-side.sbs-empty {
    background: var(--surface-hover);
  }

</style>

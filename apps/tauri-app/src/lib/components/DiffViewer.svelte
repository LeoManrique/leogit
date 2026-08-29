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
    /** The path this file was renamed from, from the status entry rather than
     *  from the diff: both reads pathspec-limit to the file's current path, so
     *  git never sees the deleted counterpart and reports a rename as a plain
     *  add. `file_diff`'s two paths describe the same file twice, or carry an
     *  absent side, and can't answer this. */
    origPath?: string | null
    showSelection?: boolean
    syntaxHighlighting?: boolean
    /** Which segment the header's layout control shows as pressed — the
     *  choice, which answers on the click. What the *body* renders is decided
     *  by the loaded pairing (`showSplit` below), a re-read later, so the rows
     *  on screen stay put until their replacement lands. */
    sideBySide?: boolean
    tabSize?: number
    /** Chosen from this header rather than from Settings — the arrangement is
     *  a property of the diff being read, so the control belongs on it. The
     *  owner persists it; core builds the pairing only for the layout that
     *  asked, so the answer arrives with the re-read. */
    onLayoutChange?: (sideBySide: boolean) => void
    onLineToggle?: (lineIndex: number) => void
    onHunkToggle?: (hunkIndex: number) => void
  }

  let {
    diff = null,
    selection = null,
    blobSource = null,
    origPath = null,
    showSelection = false,
    syntaxHighlighting = true,
    sideBySide = false,
    tabSize = 4,
    onLayoutChange = () => {},
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
    // U+2212 MINUS SIGN, not a hyphen — STYLE.md's glyph for a removed line,
    // and what the native pane renders. It sits at the `+`'s optical weight
    // and width, which a hyphen does not.
    if (line.line_type === 'Delete') return '−'
    // No prefix for `\ No newline at end of file`: core keeps that marker's
    // own leading backslash in `content`, so adding one renders it twice.
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

  /*
    Which arrangement is on screen is decided by the *loaded pairing*, not by
    the setting: core builds `sbs_pairs` only for the layout that asked, so a
    freshly toggled setting arrives a whole re-read before its rows do. Reading
    the setting here would empty the pane for the length of that read, where the
    contract (FRONTEND §6.3, §7) is that what is showing stays until the
    replacement lands. A pairing is empty exactly when the unified layout was
    what was asked for — a diff with hunks always yields at least one pair, and
    one without hunks is binary or empty, both handled before this point.
  */
  const showSplit = $derived(!!diff && diff.sbs_pairs.length > 0)

  const rows = $derived.by((): DiffRow[] => {
    if (!fileDiff || showSplit || fileDiff.is_binary) return []
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
    if (!showSplit || !diff) return []
    return diff.sbs_pairs.map((p, i) => ({ pair: p, key: `S-${i}` }))
  })

  /*
    Which diff is on screen: its paths *and* where it was read from. The commit
    is part of the identity because one path in two commits is two different
    diffs — stepping through History with the same file selected would
    otherwise keep the reader's offset from the previous commit's version of
    it. NUL joins the parts, the one byte a git path cannot contain.

    A `$derived` rather than a read inside the effect: the parent rebuilds
    `blobSource` inline on every status poll, and a derived string that comes
    back equal stops there instead of waking the effect.
  */
  const renderedDiffKey = $derived(
    fileDiff
      ? [
          blobSource?.kind === 'commit' ? blobSource.sha : '',
          fileDiff.old_path,
          fileDiff.new_path,
        ].join('\u0000')
      : null
  )

  // Reset scroll position when the user opens a different diff — leaving
  // scrollTop pinned to the previous one's offset would land them mid-file in
  // the new one (often past its end). Deliberately *not* on a layout change,
  // a whitespace toggle or an edit landing: those are the same diff, and
  // FRONTEND §6.3's contract keeps the reader's offset however it was re-read
  // — which is also why the two arrangements share one scroll container below.
  let lastDiffKey = $state<string | null>(null)
  $effect(() => {
    const key = renderedDiffKey
    if (key !== lastDiffKey) {
      lastDiffKey = key
      if (scrollContainer) scrollContainer.scrollTop = 0
    }
  })
</script>

{#if fileDiff}
  <div class="diff-viewer" style="--tab-size: {tabSize}">
    <div class="file-header">
      {#if origPath && origPath !== fileDiff.new_path}
        <span class="old-path">{origPath}</span>
        <span class="arrow">→</span>
      {/if}
      <span class="new-path">{fileDiff.new_path || fileDiff.old_path}</span>
      <div class="header-trailing">
        {#if diff && (diff.additions > 0 || diff.deletions > 0)}
          <span class="line-counts">
            {#if diff.additions > 0}<span class="add-count">+{diff.additions}</span>{/if}
            {#if diff.deletions > 0}<span class="del-count">−{diff.deletions}</span>{/if}
          </span>
        {/if}
        {#if !fileDiff.is_binary}
          <!-- A binary file has no lines to arrange, and a control that does
               nothing is worse than no control. -->
          <div class="layout-toggle" role="group" aria-label="Diff layout">
            <button
              class="layout-btn"
              class:active={!sideBySide}
              aria-pressed={!sideBySide}
              title="Unified — one column of changes"
              aria-label="Unified"
              onclick={() => onLayoutChange(false)}
            >
              <svg
                width="15"
                height="15"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.3"
                stroke-linecap="round"
                aria-hidden="true"
              >
                <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
                <path d="M4.5 6h7M4.5 8.5h7M4.5 11h4" />
              </svg>
            </button>
            <button
              class="layout-btn"
              class:active={sideBySide}
              aria-pressed={sideBySide}
              title="Split — old and new side by side"
              aria-label="Split"
              onclick={() => onLayoutChange(true)}
            >
              <svg
                width="15"
                height="15"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.3"
                stroke-linecap="round"
                aria-hidden="true"
              >
                <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
                <path d="M8 2.5v11" />
              </svg>
            </button>
          </div>
        {/if}
      </div>
    </div>

    {#if fileDiff.is_binary}
      <div class="binary-state">
        <p>This binary file has changed.</p>
      </div>
    {:else}
      <!--
        One scroll container for both arrangements, never one per branch:
        swapping the branch would destroy the element the offset lives on
        and drop the reader at the top of the file every time they changed
        the layout, which is the same file (FRONTEND §6.3).
      -->
      <div class="diff-body" bind:this={scrollContainer}>
        {#if showSplit}
          {#each sbsRows as row (row.key)}
            {@const left = row.pair.left !== null ? flatLines[row.pair.left] : null}
            {@const right = row.pair.right !== null ? flatLines[row.pair.right] : null}
            {#if row.pair.is_hunk_header && left}
              <div class="hunk-header sbs-hunk-header">
                <span class="hunk-text">{left.text ?? left.content}</span>
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
        {:else}
          {#each rows as row (row.key)}
            {#if row.kind === 'header'}
              <!--
                Two headers, not one with conditional attributes: while per-line
                staging is unwired, `showSelection` is false everywhere and the
                interactive form costs real usability — a focusable no-op button
                per hunk, one tab stop each on an unvirtualized list, whose text
                could not be selected because a control does not select. The
                scaffolding stays for when staging is finished; its cost does not.
              -->
              {#if showSelection}
                <div
                  class="hunk-header interactive"
                  onclick={(e) => { if (e.shiftKey) onHunkToggle(row.hunkIdx) }}
                  onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.shiftKey) { e.preventDefault(); onHunkToggle(row.hunkIdx) } }}
                  role="button"
                  tabindex="0"
                >
                  <span class="hunk-text">{row.line.text ?? row.line.content}</span>
                  <span class="hunk-hint">Shift+click for hunk</span>
                </div>
              {:else}
                <div class="hunk-header">
                  <span class="hunk-text">{row.line.text ?? row.line.content}</span>
                </div>
              {/if}
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
        {/if}
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

  /* The header's trailing cluster: the totals, then the layout control. One
     `margin-left: auto` for the pair, so neither pushes the other around. */
  .header-trailing {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }

  /* Per-file +adds/-dels totals. */
  .line-counts {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .add-count { color: var(--diff-add-fg); }
  .del-count { color: var(--diff-remove-fg); }

  /* Two joined segments rather than one toggling glyph: the arrangement has
     two named states, and a control that shows only the one you are in leaves
     the reader working out whether the icon is where they are or where they
     would go. STYLE.md's *Segmented controls* treatment, at icon size. */
  .layout-toggle {
    display: inline-flex;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    overflow: hidden;
  }

  .layout-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 0;
    cursor: pointer;
    transition:
      color 100ms ease,
      background 100ms ease;
  }

  .layout-btn + .layout-btn {
    border-left: 1px solid var(--border-strong);
  }

  .layout-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .layout-btn.active {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

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
  }

  /* Only the staging form is a control; the plain band is text you can select
     like the rest of the diff. */
  .hunk-header.interactive {
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

  /*
    Out of the selection along with the gutter: dragging across a diff should
    put the file's own lines on the clipboard, not `+`/`−` glyphs the viewer
    added. (The whole-model copy that also fixes side-by-side interleaving is
    core's `copy_diff_text`, waiting on a Copy action to call it; this is the
    half that costs nothing and helps every ordinary drag-select today.)
  */
  .line-prefix {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5em;
    padding: 0 4px;
    flex-shrink: 0;
    font-weight: 500;
    user-select: none;
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

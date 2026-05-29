<script lang="ts">
  // `shiki/bundle/web` is a curated subset — it omits Go, Rust, C, and many
  // other languages we actually want. `bundle/full` ships them all (lazy-loaded
  // on demand by `createHighlighter`), which is what we need for a code-host
  // app where the user can open any file type.
  import { createHighlighter, type Highlighter } from 'shiki/bundle/full'
  import type { FileDiff, DiffSelection, DiffLine } from '$lib/api/commands'

  let highlighterPromise: Promise<Highlighter> | null = null
  function getHighlighter(): Promise<Highlighter> {
    if (!highlighterPromise) {
      highlighterPromise = createHighlighter({
        themes: ['github-dark', 'github-light'],
        langs: [
          'javascript', 'typescript', 'tsx', 'jsx', 'svelte', 'vue', 'html', 'css', 'scss',
          'json', 'yaml', 'toml', 'markdown', 'bash', 'python', 'rust', 'go', 'java',
          'c', 'cpp', 'csharp', 'php', 'ruby', 'sql', 'xml',
        ],
      })
    }
    return highlighterPromise
  }

  interface Props {
    fileDiff: FileDiff | null
    selection: DiffSelection | null
    repoPath: string
    showSelection?: boolean
    syntaxHighlighting?: boolean
    sideBySide?: boolean
    tabSize?: number
    onLineToggle?: (lineIndex: number) => void
    onHunkToggle?: (hunkIndex: number) => void
  }

  let {
    fileDiff = null,
    selection = null,
    showSelection = false,
    syntaxHighlighting = true,
    sideBySide = false,
    tabSize = 4,
    onLineToggle = () => {},
    onHunkToggle = () => {},
  }: Props = $props()

  let highlightedHtml = $state<string[]>([])
  let theme = $state<'github-dark' | 'github-light'>('github-dark')

  $effect(() => {
    if (typeof document !== 'undefined') {
      const t = document.documentElement.dataset.theme
      theme = t === 'light' ? 'github-light' : 'github-dark'
    }
  })

  function getLanguageFromPath(path: string): string {
    if (!path) return 'plaintext'
    const ext = path.split('.').pop()?.toLowerCase() || ''
    const langMap: Record<string, string> = {
      js: 'javascript', ts: 'typescript', tsx: 'tsx', jsx: 'jsx',
      py: 'python', rs: 'rust', go: 'go', java: 'java',
      cpp: 'cpp', c: 'c', cs: 'csharp', php: 'php',
      rb: 'ruby', swift: 'swift', kt: 'kotlin',
      sh: 'bash', bash: 'bash', zsh: 'bash',
      html: 'html', htm: 'html', css: 'css', scss: 'scss',
      json: 'json', xml: 'xml', yml: 'yaml', yaml: 'yaml',
      toml: 'toml', sql: 'sql', md: 'markdown', svelte: 'svelte', vue: 'vue',
    }
    return langMap[ext] || 'plaintext'
  }

  function escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  }

  /*
    Intra-line overlay (Relay → Metrics inside an otherwise identical line):
    the backend annotates each paired Delete/Add line with the character range
    that actually changed. We layer that range on top of Shiki's tokens so the
    syntax colour stays intact and only the changed substring gets the brighter
    backplate. When Shiki isn't running (no language match, or
    syntax_highlighting=off), the base HTML is plain escaped text and the same
    layering treats it as one big token.
  */
  type LineToken = { text: string; color?: string }

  function decodeHtmlEntities(s: string): string {
    return s
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&amp;/g, '&')
  }

  function parseShikiLineHtml(html: string, fallback: string): LineToken[] {
    // Shiki emits each line as `<span class="line">…token spans…</span>`.
    const lineMatch = html.match(/^<span class="line">([\s\S]*)<\/span>$/)
    const inner = lineMatch ? lineMatch[1] : html
    const tokens: LineToken[] = []
    const re = /<span(?:\s+style="([^"]*)")?>([^<]*)<\/span>/g
    let m: RegExpExecArray | null
    while ((m = re.exec(inner)) !== null) {
      const styleAttr = m[1] ?? ''
      const colorMatch = styleAttr.match(/color:\s*([^;]+)/)
      const color = colorMatch ? colorMatch[1].trim() : undefined
      const text = decodeHtmlEntities(m[2])
      if (text.length > 0) tokens.push({ text, color })
    }
    // No token spans found — treat the whole HTML as plain escaped text and
    // rebuild from the raw content so character indices line up.
    if (tokens.length === 0) return [{ text: fallback }]
    return tokens
  }

  function emitSpan(text: string, color: string | undefined, cssClass: string | null): string {
    const classAttr = cssClass ? ` class="${cssClass}"` : ''
    const styleAttr = color ? ` style="color:${color}"` : ''
    return `<span${classAttr}${styleAttr}>${escapeHtml(text)}</span>`
  }

  function overlayIntraLine(
    html: string,
    content: string,
    range: { start: number; length: number },
    cssClass: string,
  ): string {
    const tokens = parseShikiLineHtml(html, content)
    const intraStart = range.start
    const intraEnd = range.start + range.length
    let result = ''
    let pos = 0
    for (const tok of tokens) {
      const chars = [...tok.text] // code points so multi-byte chars don't split
      const tokStart = pos
      const tokEnd = pos + chars.length
      const overlapStart = Math.max(tokStart, intraStart)
      const overlapEnd = Math.min(tokEnd, intraEnd)
      if (overlapStart >= overlapEnd) {
        result += emitSpan(tok.text, tok.color, null)
      } else {
        const pre = chars.slice(0, overlapStart - tokStart).join('')
        const mid = chars.slice(overlapStart - tokStart, overlapEnd - tokStart).join('')
        const post = chars.slice(overlapEnd - tokStart).join('')
        if (pre) result += emitSpan(pre, tok.color, null)
        if (mid) result += emitSpan(mid, tok.color, cssClass)
        if (post) result += emitSpan(post, tok.color, null)
      }
      pos = tokEnd
    }
    return result
  }

  async function computeBaseHtml(allLines: DiffLine[], path: string): Promise<string[]> {
    if (!syntaxHighlighting) return allLines.map((l) => escapeHtml(l.content))
    const lang = getLanguageFromPath(path)
    if (lang === 'plaintext') return allLines.map((l) => escapeHtml(l.content))
    try {
      const hl = await getHighlighter()
      if (!hl.getLoadedLanguages().includes(lang as any)) {
        return allLines.map((l) => escapeHtml(l.content))
      }
      const text = allLines.map((l) => l.content).join('\n')
      const html = hl.codeToHtml(text, { lang: lang as any, theme })
      const match = html.match(/<code[^>]*>([\s\S]*?)<\/code>/)
      if (!match) return allLines.map((l) => escapeHtml(l.content))
      const lines = match[1].split('\n')
      while (lines.length < allLines.length) lines.push('')
      return lines.slice(0, allLines.length)
    } catch {
      return allLines.map((l) => escapeHtml(l.content))
    }
  }

  function applyIntraLineOverlay(base: string[], lines: DiffLine[]): string[] {
    return base.map((html, i) => {
      const line = lines[i]
      const intra = line?.intra_line_diff
      if (!intra || intra.length === 0) return html
      const cls =
        line.line_type === 'Add' ? 'diff-intra-add'
        : line.line_type === 'Delete' ? 'diff-intra-remove'
        : null
      if (!cls) return html
      return overlayIntraLine(html, line.content, intra, cls)
    })
  }

  async function highlightAll() {
    if (!fileDiff) {
      highlightedHtml = []
      return
    }
    const allLines: DiffLine[] = []
    for (const h of fileDiff.hunks) allLines.push(...h.lines)
    const base = await computeBaseHtml(allLines, fileDiff.new_path || fileDiff.old_path)
    highlightedHtml = applyIntraLineOverlay(base, allLines)
  }

  $effect(() => {
    // Track props that should trigger a re-highlight
    void fileDiff
    void syntaxHighlighting
    void theme
    if (fileDiff) highlightAll()
    else highlightedHtml = []
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

  function flatIndex(hunkIdx: number, lineIdx: number): number {
    if (!fileDiff) return 0
    let idx = 0
    for (let i = 0; i < hunkIdx; i++) idx += fileDiff.hunks[i].lines.length
    return idx + lineIdx
  }

  // Build paired rows for side-by-side: { left, right, leftIdx, rightIdx }
  type Pair = {
    left: DiffLine | null
    right: DiffLine | null
    leftGlobalIdx: number | null
    rightGlobalIdx: number | null
    isHunkHeader: boolean
    hunkIdx: number
  }

  function buildPairs(): Pair[] {
    if (!fileDiff) return []
    const pairs: Pair[] = []

    for (let hunkIdx = 0; hunkIdx < fileDiff.hunks.length; hunkIdx++) {
      const hunk = fileDiff.hunks[hunkIdx]
      // Collect runs of deletes and adds within the hunk and pair them
      let i = 0
      while (i < hunk.lines.length) {
        const line = hunk.lines[i]
        const globalI = flatIndex(hunkIdx, i)

        if (line.line_type === 'Hunk') {
          pairs.push({ left: line, right: line, leftGlobalIdx: globalI, rightGlobalIdx: globalI, isHunkHeader: true, hunkIdx })
          i++
          continue
        }

        if (line.line_type === 'Context') {
          pairs.push({ left: line, right: line, leftGlobalIdx: globalI, rightGlobalIdx: globalI, isHunkHeader: false, hunkIdx })
          i++
          continue
        }

        if (line.line_type === 'NoNewline') {
          // Attach to last row
          const last = pairs[pairs.length - 1]
          if (last) {
            if (last.left && last.left.line_type !== 'Add') last.left = { ...last.left, text: last.left.text + ' (no newline)' }
            if (last.right && last.right.line_type !== 'Delete') last.right = { ...last.right, text: last.right.text + ' (no newline)' }
          }
          i++
          continue
        }

        // Collect a delete run and an add run
        const deletes: { line: DiffLine; idx: number }[] = []
        const adds: { line: DiffLine; idx: number }[] = []
        while (i < hunk.lines.length && hunk.lines[i].line_type === 'Delete') {
          deletes.push({ line: hunk.lines[i], idx: flatIndex(hunkIdx, i) })
          i++
        }
        while (i < hunk.lines.length && hunk.lines[i].line_type === 'Add') {
          adds.push({ line: hunk.lines[i], idx: flatIndex(hunkIdx, i) })
          i++
        }

        const max = Math.max(deletes.length, adds.length)
        for (let k = 0; k < max; k++) {
          pairs.push({
            left: deletes[k]?.line ?? null,
            right: adds[k]?.line ?? null,
            leftGlobalIdx: deletes[k]?.idx ?? null,
            rightGlobalIdx: adds[k]?.idx ?? null,
            isHunkHeader: false,
            hunkIdx,
          })
        }
      }
    }
    return pairs
  }

  let pairs = $derived(sideBySide ? buildPairs() : [])
</script>

{#if fileDiff}
  <div class="diff-viewer" style="--tab-size: {tabSize}">
    <div class="file-header">
      {#if fileDiff.old_path && fileDiff.old_path !== fileDiff.new_path}
        <span class="old-path">{fileDiff.old_path}</span>
        <span class="arrow">→</span>
      {/if}
      <span class="new-path">{fileDiff.new_path || fileDiff.old_path}</span>
    </div>

    {#if fileDiff.is_binary}
      <div class="binary-state">
        <p>This binary file has changed.</p>
      </div>
    {:else if sideBySide}
      <div class="sbs-container">
        {#each pairs as pair, pairIdx (pairIdx)}
          {#if pair.isHunkHeader && pair.left}
            <div class="hunk-header sbs-hunk-header">
              <span class="hunk-text">{pair.left.text}</span>
            </div>
          {:else}
            <div class="sbs-row">
              <div class="sbs-side sbs-left {pair.left ? lineTypeClass(pair.left.line_type) : 'sbs-empty'}">
                <span class="line-number">{pair.left?.old_line_no ?? ''}</span>
                <span class="line-prefix">{pair.left ? linePrefix(pair.left) : ' '}</span>
                <span class="line-content">
                  {#if pair.left && pair.leftGlobalIdx !== null && highlightedHtml[pair.leftGlobalIdx]}
                    {@html highlightedHtml[pair.leftGlobalIdx]}
                  {:else if pair.left}
                    {pair.left.content}
                  {/if}
                </span>
              </div>
              <div class="sbs-side sbs-right {pair.right ? lineTypeClass(pair.right.line_type) : 'sbs-empty'}">
                <span class="line-number">{pair.right?.new_line_no ?? ''}</span>
                <span class="line-prefix">{pair.right ? linePrefix(pair.right) : ' '}</span>
                <span class="line-content">
                  {#if pair.right && pair.rightGlobalIdx !== null && highlightedHtml[pair.rightGlobalIdx]}
                    {@html highlightedHtml[pair.rightGlobalIdx]}
                  {:else if pair.right}
                    {pair.right.content}
                  {/if}
                </span>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="hunks-container">
        {#each fileDiff.hunks as hunk, hunkIndex}
          {#each hunk.lines as line, lineIndex}
            {@const globalIdx = flatIndex(hunkIndex, lineIndex)}
            {#if line.line_type === 'Hunk'}
              <div
                class="hunk-header"
                onclick={(e) => { if (e.shiftKey) onHunkToggle(hunkIndex) }}
                onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.shiftKey) { e.preventDefault(); onHunkToggle(hunkIndex) } }}
                role="button"
                tabindex="0"
              >
                <span class="hunk-text">{line.text}</span>
                {#if showSelection}<span class="hunk-hint">Shift+click for hunk</span>{/if}
              </div>
            {:else}
              <div class="diff-line {lineTypeClass(line.line_type)}">
                <span class="line-number old">{line.old_line_no ?? ''}</span>
                <span class="line-number new">{line.new_line_no ?? ''}</span>
                <span class="line-prefix">{linePrefix(line)}</span>
                <span class="line-content">
                  {#if highlightedHtml[globalIdx]}
                    {@html highlightedHtml[globalIdx]}
                  {:else}
                    {line.content}
                  {/if}
                </span>
                {#if showSelection && (line.line_type === 'Add' || line.line_type === 'Delete')}
                  <button
                    class="selection-dot"
                    class:selected={isLineSelected(globalIdx)}
                    onclick={() => onLineToggle(globalIdx)}
                    title="Toggle line selection"
                    aria-label="Toggle line selection"
                  ></button>
                {/if}
              </div>
            {/if}
          {/each}
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

  .hunks-container,
  .sbs-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .hunk-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 12px;
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
    align-items: flex-start;
    min-height: 18px;
    border-bottom: 1px solid transparent;
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
    white-space: pre-wrap;
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  .line-content :global(.shiki) {
    background: transparent !important;
    color: inherit;
    margin: 0;
    padding: 0;
  }

  .line-content :global(pre),
  .line-content :global(code) {
    margin: 0;
    padding: 0;
    background: none;
    font-family: inherit;
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
    min-height: 18px;
  }

  .sbs-side {
    display: flex;
    align-items: flex-start;
    overflow: hidden;
    border-right: 1px solid var(--border-inactive);
  }

  .sbs-side.sbs-right {
    border-right: none;
  }

  .sbs-side.sbs-empty {
    background: var(--surface-hover);
  }
</style>

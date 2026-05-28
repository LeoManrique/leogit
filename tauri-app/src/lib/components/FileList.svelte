<script lang="ts">
  import { tick } from 'svelte'
  import type { FileEntry } from '$lib/api/commands'
  import PathText from './PathText.svelte'

  interface Props {
    files: FileEntry[]
    selectedFiles?: Set<string>
    activeFile?: FileEntry | null
    showCheckbox?: boolean
    onActivate: (file: FileEntry) => void
    onToggle?: (file: FileEntry) => void
    /**
     * Select-all toggle for the master checkbox. Receives `true` to include
     * every file in the commit and `false` to exclude every file. Omit to
     * hide the header row (e.g. on the commit detail view).
     */
    onToggleAll?: (selectAll: boolean) => void
    /**
     * Apply the same include/exclude state to a batch of files in one shot.
     * Used by shift+click on a checkbox (range toggle) and Space on a
     * multi-row selection. Omit if the caller doesn't need either gesture.
     */
    onBulkToggle?: (paths: string[], include: boolean) => void
  }

  let {
    files = [],
    selectedFiles = new Set(),
    activeFile = null,
    showCheckbox = true,
    onActivate,
    onToggle = () => {},
    onToggleAll,
    onBulkToggle,
  }: Props = $props()

  // Visual multi-row selection. Independent of commit inclusion — this is the
  // user's mouse-/keyboard-driven highlight, not the staged set. Lives local
  // to FileList so it survives tab switches (component stays mounted) but
  // resets when FileList unmounts (e.g. on repo switch).
  let rowSelection = $state<Set<string>>(new Set())
  // Anchor for shift+click on the row BODY. Set by plain clicks; shift+clicks
  // re-extend from this anchor without moving it, matching Finder semantics.
  let rowAnchor = $state<string | null>(null)
  // Anchor for shift+click on a CHECKBOX. Tracked independently so the two
  // gestures don't bleed into each other.
  let checkboxAnchor = $state<string | null>(null)

  // ---- Virtualization --------------------------------------------------
  // Render only the rows currently in view (plus a small buffer) instead of
  // pushing every changed file into the DOM. Without this, switching back to
  // the Changes tab with a 1000+ file changeset blocks the main thread for
  // hundreds of ms while the browser lays out every row. Mirrors the
  // approach GitHub Desktop takes via react-virtualized's Grid.
  const ROW_HEIGHT = 24
  const BUFFER_ROWS = 8

  let viewportEl = $state<HTMLDivElement | null>(null)
  let scrollTop = $state(0)
  let viewportHeight = $state(0)

  $effect(() => {
    if (!viewportEl) return
    viewportHeight = viewportEl.clientHeight
    const ro = new ResizeObserver(() => {
      if (viewportEl) viewportHeight = viewportEl.clientHeight
    })
    ro.observe(viewportEl)
    return () => ro.disconnect()
  })

  function onViewportScroll() {
    if (!viewportEl) return
    scrollTop = viewportEl.scrollTop
  }

  const startIdx = $derived(
    Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_ROWS),
  )
  const endIdx = $derived(
    Math.min(
      files.length,
      Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + BUFFER_ROWS,
    ),
  )
  const visibleFiles = $derived(files.slice(startIdx, endIdx))
  const totalHeight = $derived(files.length * ROW_HEIGHT)

  /** Files between `anchorPath` and `clickedPath` inclusive, ordered as displayed. */
  function rangeBetween(anchorPath: string | null, clickedPath: string): FileEntry[] {
    const fallback = files.find((f) => f.path === clickedPath)
    if (!anchorPath) return fallback ? [fallback] : []
    const a = files.findIndex((f) => f.path === anchorPath)
    const b = files.findIndex((f) => f.path === clickedPath)
    if (a < 0 || b < 0) return fallback ? [fallback] : []
    const [from, to] = a <= b ? [a, b] : [b, a]
    return files.slice(from, to + 1)
  }

  function handleRowClick(e: MouseEvent, file: FileEntry) {
    if (e.shiftKey && rowAnchor) {
      const range = rangeBetween(rowAnchor, file.path)
      rowSelection = new Set(range.map((f) => f.path))
      // Anchor stays put — user can keep re-extending from the same spot.
      onActivate(file)
      return
    }
    rowAnchor = file.path
    rowSelection = new Set([file.path])
    onActivate(file)
  }

  /**
   * Move keyboard focus + activation to another row by index. Mirrors GH
   * Desktop's `List.moveSelection` / `scrollRowToVisible`: focus the new row
   * (so the next arrow press continues navigating) and nudge it into view.
   *
   * With virtualization the target row may not be in the DOM yet, so we
   * first compute and apply the scrollTop that would bring it into view,
   * await Svelte's re-render via `tick()`, then focus the now-rendered row.
   */
  async function focusRowAt(index: number) {
    const clamped = Math.max(0, Math.min(files.length - 1, index))
    const next = files[clamped]
    if (!next) return
    rowAnchor = next.path
    rowSelection = new Set([next.path])
    onActivate(next)

    if (viewportEl) {
      const top = clamped * ROW_HEIGHT
      const bottom = top + ROW_HEIGHT
      const vh = viewportEl.clientHeight
      let newScroll = viewportEl.scrollTop
      if (top < newScroll) newScroll = top
      else if (bottom > newScroll + vh) newScroll = bottom - vh
      if (newScroll !== viewportEl.scrollTop) {
        viewportEl.scrollTop = newScroll
        // Sync state synchronously so derived ranges update before tick().
        // Otherwise scroll events arrive asynchronously and we'd focus before
        // the new row is rendered.
        scrollTop = newScroll
      }
    }

    await tick()

    const el = viewportEl?.querySelector<HTMLDivElement>(
      `[data-file-row-index="${clamped}"]`,
    )
    if (!el) return
    el.focus({ preventScroll: true })
  }

  function handleCheckboxClick(e: MouseEvent, file: FileEntry) {
    e.stopPropagation()
    if (e.shiftKey && checkboxAnchor && checkboxAnchor !== file.path) {
      // Cancel the native toggle — we'll drive every checkbox in the range,
      // including this one, via the bulk callback.
      e.preventDefault()
      const willInclude = !selectedFiles.has(file.path)
      const range = rangeBetween(checkboxAnchor, file.path)
      onBulkToggle?.(range.map((f) => f.path), willInclude)
      checkboxAnchor = file.path
      return
    }
    // Plain click: native toggle fires; we just record the anchor + propagate.
    checkboxAnchor = file.path
    onToggle(file)
  }

  // Drop stale paths after a commit / discard / external `git rm` removes
  // files from the working tree. Anchors that point at a file no longer in
  // the list go null so the next plain click reseats them cleanly.
  $effect(() => {
    const present = new Set(files.map((f) => f.path))
    if (rowSelection.size > 0) {
      const next = new Set([...rowSelection].filter((p) => present.has(p)))
      if (next.size !== rowSelection.size) rowSelection = next
    }
    if (rowAnchor && !present.has(rowAnchor)) rowAnchor = null
    if (checkboxAnchor && !present.has(checkboxAnchor)) checkboxAnchor = null
  })

  let masterCheckbox = $state<HTMLInputElement | null>(null)

  const allSelected = $derived(
    files.length > 0 && files.every((f) => selectedFiles.has(f.path)),
  )
  const isIndeterminate = $derived(
    !allSelected && files.some((f) => selectedFiles.has(f.path)),
  )

  // `indeterminate` is a DOM property, not an attribute — Svelte can't render
  // it from markup. Reflect it via an effect whenever the derived flag flips.
  $effect(() => {
    if (masterCheckbox) masterCheckbox.indeterminate = isIndeterminate
  })

  function toggleAll(e?: Event) {
    e?.stopPropagation()
    onToggleAll?.(!allSelected)
  }

  function getStatusColor(status: string): string {
    switch (status) {
      case 'New':
        return 'var(--status-green)'
      case 'Modified':
        return 'var(--status-yellow)'
      case 'Deleted':
        return 'var(--status-red)'
      case 'Renamed':
        return 'var(--status-blue)'
      case 'Conflicted':
        return 'var(--status-red)'
      default:
        return 'var(--text-secondary)'
    }
  }

  function getStatusLabel(status: string): string {
    switch (status) {
      case 'New':
        return 'A'
      case 'Modified':
        return 'M'
      case 'Deleted':
        return 'D'
      case 'Renamed':
        return 'R'
      case 'Conflicted':
        return 'U'
      default:
        return '?'
    }
  }

</script>

<div class="file-list">
  {#if showCheckbox && onToggleAll && files.length > 0}
    <div
      class="file-row select-all-row"
      onclick={toggleAll}
      role="button"
      tabindex="0"
      onkeydown={(e) => {
        if (e.key === ' ' || e.key === 'Enter') {
          e.preventDefault()
          toggleAll()
        }
      }}
      title={allSelected ? 'Deselect all files' : 'Select all files'}
    >
      <input
        bind:this={masterCheckbox}
        type="checkbox"
        class="file-checkbox"
        checked={allSelected}
        aria-label={allSelected ? 'Deselect all files' : 'Select all files'}
        onclick={(e) => {
          e.stopPropagation()
          toggleAll()
        }}
        onkeydown={(e) => e.stopPropagation()}
      />
      <span class="select-all-label">
        {files.length} changed file{files.length === 1 ? '' : 's'}
      </span>
    </div>
  {/if}

  <div
    class="rows-viewport"
    bind:this={viewportEl}
    onscroll={onViewportScroll}
  >
    {#if files.length === 0}
      <div class="empty-state">
        <p>No changes</p>
      </div>
    {:else}
      <div class="rows-spacer" style="height: {totalHeight}px;">
        {#each visibleFiles as file, i (file.path)}
          {@const fileIndex = startIdx + i}
          {@const isSelected = selectedFiles.has(file.path)}
          {@const isActive = activeFile?.path === file.path}
          {@const isRowSelected = rowSelection.has(file.path)}
          <div
            class="file-row virtual-row"
            class:active={isActive}
            class:included={isSelected}
            class:row-selected={isRowSelected}
            data-file-row-index={fileIndex}
            style="top: {fileIndex * ROW_HEIGHT}px;"
            onclick={(e) => handleRowClick(e, file)}
            role="button"
            tabindex="0"
            onkeydown={(e) => {
              // Home/End first — on macOS those are Cmd+ArrowUp / Cmd+ArrowDown,
              // so they must beat the plain Arrow branches below.
              if (e.key === 'Home' || (e.key === 'ArrowUp' && e.metaKey)) {
                e.preventDefault()
                focusRowAt(0)
              } else if (e.key === 'End' || (e.key === 'ArrowDown' && e.metaKey)) {
                e.preventDefault()
                focusRowAt(files.length - 1)
              } else if (e.key === 'ArrowDown') {
                // Navigate to next file. Default browser behavior is to scroll
                // the container — block it so the selection moves instead, matching
                // the desktop list pattern.
                e.preventDefault()
                focusRowAt(fileIndex + 1)
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                focusRowAt(fileIndex - 1)
              } else if (e.key === 'Enter') {
                e.preventDefault()
                onActivate(file)
              } else if (e.key === ' ' && showCheckbox) {
                e.preventDefault()
                // Bulk-toggle the visual multi-selection when one exists and the
                // focused row is part of it; otherwise fall back to single toggle.
                // Match the master-checkbox rule: any-excluded → include all, else
                // exclude all.
                if (rowSelection.size > 1 && rowSelection.has(file.path) && onBulkToggle) {
                  const paths = [...rowSelection]
                  const anyExcluded = paths.some((p) => !selectedFiles.has(p))
                  onBulkToggle(paths, anyExcluded)
                } else {
                  onToggle(file)
                }
              }
            }}
          >
            {#if showCheckbox}
              <input
                type="checkbox"
                class="file-checkbox"
                checked={isSelected}
                aria-label={isSelected ? `Exclude ${file.path} from commit` : `Include ${file.path} in commit`}
                onclick={(e) => handleCheckboxClick(e, file)}
                onkeydown={(e) => e.stopPropagation()}
              />
            {/if}

            <div class="status-badge" style="color: {getStatusColor(file.status)}">
              {getStatusLabel(file.status)}
            </div>

            {#if file.orig_path}
              <div class="file-info" title={file.path}>
                <span class="orig">{file.orig_path}</span>
                <span class="arrow">→</span>
                <PathText path={file.path} />
              </div>
            {:else}
              <PathText path={file.path} />
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .file-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-secondary);
    padding-top: 4px;
    min-height: 0;
  }

  /*
    The actual scroller. Rows are absolutely positioned inside .rows-spacer,
    which carries the full virtual height so the scrollbar tracks the true
    document size.
  */
  .rows-viewport {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    padding: 0 6px 4px 6px;
    min-height: 0;
  }

  .rows-spacer {
    position: relative;
    width: 100%;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-faint);
    font-size: 13px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px;
    height: 24px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 100ms ease;
  }

  /*
    Virtualized file rows are positioned by absolute `top` based on their
    index — that's how we keep the DOM small while preserving the scrollbar's
    sense of total list height. The select-all-row sits above the viewport
    in normal flow, so it keeps the default static positioning.
  */
  .virtual-row {
    position: absolute;
    left: 0;
    right: 0;
  }

  .file-row:hover {
    background: var(--surface-hover);
  }

  /*
    Visual multi-row highlight (shift+click range, mouse selection).
    Independent of commit inclusion (.included) and the diff-displayed
    row (.active). Subtle so a sole selection still reads as "marked"
    without competing with the active row's heavier backplate.
  */
  .file-row.row-selected {
    background: var(--surface-hover);
  }

  .file-row.active {
    background: var(--bg-tertiary);
  }

  /*
    Master select-all row, modelled on GH Desktop's "N changed files" header.
    Slightly muted vs. file rows so it reads as a header, not another file.
  */
  .select-all-row {
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 500;
    border-bottom: 1px solid var(--border-inactive);
    border-radius: 0;
    margin: 0 6px 2px 6px;
  }

  .select-all-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-checkbox {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    cursor: pointer;
    accent-color: var(--border-active);
    margin: 0;
  }

  .file-checkbox:focus-visible {
    outline: 2px solid var(--border-active);
    outline-offset: 2px;
  }

  .status-badge {
    width: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .file-info {
    display: flex;
    align-items: baseline;
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    white-space: nowrap;
    gap: 4px;
  }

  .orig {
    color: var(--text-muted);
    text-decoration: line-through;
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .arrow {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .file-row.included :global(.filename) {
    font-weight: 500;
  }
</style>

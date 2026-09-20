<script lang="ts">
  import { untrack } from 'svelte'
  import type { FileEntry } from '$lib/api/commands'
  import { fileStatusStyles } from '$lib/stores/config'
  import { revealLabel, fileExtension, type FileContextActions } from '$lib/services/fileActions'
  import {
    EMPTY_SELECTION,
    activeKey,
    applyGesture,
    clickGesture,
    contextTargets,
    isSelectAllChord,
    keyGesture,
    rangeBetween,
    reseated,
    rowIndexForKey,
    selectAll,
    type ListSelection,
    type SelectionGesture,
  } from '$lib/utils/listSelection'
  import { focusVirtualRow } from '$lib/utils/virtualList'
  import PathText from './PathText.svelte'
  import ContextMenu, { MENU_SEPARATOR, type ContextMenuItem } from './ContextMenu.svelte'

  interface Props {
    files: FileEntry[]
    selectedFiles?: Set<string>
    activeFile?: FileEntry | null
    showCheckbox?: boolean
    /**
     * Right-click actions for each file row. Supplied only by the working-tree
     * Changes list; omit it (e.g. on the commit-detail file list) and rows keep
     * the native menu with no custom context menu.
     */
    contextActions?: FileContextActions
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
    contextActions,
    onActivate,
    onToggle = () => {},
    onToggleAll,
    onBulkToggle,
  }: Props = $props()

  // Shown on a dirty submodule's disabled checkbox/badge. Its inner changes
  // aren't a parent-repo change, so they must be committed in the submodule.
  const SUBMODULE_DIRTY_HINT =
    'This submodule has uncommitted changes that must be committed inside the submodule before they can be part of this repository.'

  // Visual multi-row selection. Independent of commit inclusion — this is the
  // user's mouse-/keyboard-driven highlight, not the staged set. Lives local
  // to FileList so it survives tab switches (component stays mounted) but
  // resets when FileList unmounts (e.g. on repo switch). The rules it follows
  // are `listSelection.ts`'s, shared with the commit list; `raw` because a
  // selection is a value that is replaced, never mutated.
  let rowSelection = $state.raw<ListSelection>(EMPTY_SELECTION)
  // Anchor for shift+click on a CHECKBOX. Tracked apart from the row
  // selection's own anchor so the two gestures don't bleed into each other.
  let checkboxAnchor = $state<string | null>(null)

  /** The list as it is drawn, by key — what every selection rule reads. */
  const paths = $derived(files.map((f) => f.path))

  // ---- Virtualization --------------------------------------------------
  // Render only the rows currently in view (plus a small buffer) instead of
  // pushing every changed file into the DOM. Without this, switching back to
  // the Changes tab with a 1000+ file changeset blocks the main thread for
  // hundreds of ms while the browser lays out every row.
  //
  // ROW_HEIGHT is the row's pitch *and* its CSS height, and the two may never
  // disagree — a stylesheet row taller or shorter than the step this file
  // positions by tears the list open at every scroll offset. So it is declared
  // once here and handed to CSS as `--row-height` on the wrapper below, rather
  // than written down a second time in the style block.
  //
  // Do not spell that block's tag out in this comment. Svelte's own parser
  // reads the script as raw text and would not care, but `svelte-check`
  // (through `svelte2tsx`) scans the file for the block boundaries with a
  // lexer that has no idea what a JS comment is — a literal style or script
  // tag anywhere in here ends the script early, and the file then fails to
  // resolve with `<script> was left open` reported against its last line,
  // while `vite build` compiles it perfectly happily.
  //
  // 30px is `ChangedFileList.swift`'s row measured out. It sets no explicit
  // height, so the row is what its content asks for plus what the list style
  // gives it. Content: an `HStack(spacing: 10)` whose tallest child is the
  // 18pt `FileStatusBadge` plate (`FileStatusStyle.swift:71`) — taller than
  // the checkbox and taller than a 13pt line — under
  // `.padding(.vertical, 2)` (`ChangedFileList.swift:111`), so 22pt. The
  // remaining 8 is the per-row inset `.listStyle(.inset)`
  // (`ChangedFileList.swift:113`) adds, 4pt a side, which is the figure that
  // reconciles that 22 with the 29–30px pitch measured off the native window.
  // It is the one number here not read out of the Swift, so it is the one to
  // re-measure if the row ever looks a pixel off.
  //
  // The 6px of air it leaves around the 18px badge is what the number is
  // really for: the badge is the row's floor, and a pitch close to it reads as
  // a table of cells rather than as a list you can run your eye down. This is
  // the loudest single measurement in the Changes tab — get it wrong and the
  // rest of the row being pixel-correct does not rescue it.
  const ROW_HEIGHT = 30
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

  /**
   * Land a gesture on a row, and keep the diff pane on the row it landed on.
   *
   * The pane follows the *clicked* row — this client's rule, where the native
   * pane holds still for a multi-row selection (FRONTEND §8). The one gesture
   * with no row to follow is a toggle that took its row *out*: the pane then
   * keeps the file it had while that is still selected, and otherwise moves to
   * the first selected row.
   */
  function selectRow(file: FileEntry, gesture: SelectionGesture) {
    rowSelection = applyGesture(rowSelection, paths, file.path, gesture)
    const shown = rowSelection.keys.has(file.path)
      ? file.path
      : activeKey(rowSelection, paths, activeFile?.path ?? null)
    const target = shown === file.path ? file : files.find((f) => f.path === shown)
    if (target) onActivate(target)
  }

  /**
   * Move keyboard focus and the selection to another row by index: select it
   * (Shift extends), then bring it into view and focus it so the next arrow
   * press continues from there.
   */
  async function focusRowAt(index: number, gesture: SelectionGesture) {
    const next = files[index]
    if (!next) return
    selectRow(next, gesture)
    await focusVirtualRow({
      container: viewportEl,
      index,
      rowHeight: ROW_HEIGHT,
      rowSelector: `[data-file-row-index="${index}"]`,
      onScroll: (top) => (scrollTop = top),
    })
  }

  function handleRowKeyDown(e: KeyboardEvent, file: FileEntry, fileIndex: number) {
    const target = rowIndexForKey(e, fileIndex, files.length)
    if (target !== null) {
      // The container would scroll otherwise; move the selection instead.
      e.preventDefault()
      void focusRowAt(target, keyGesture(e))
    } else if (isSelectAllChord(e)) {
      e.preventDefault()
      rowSelection = selectAll(rowSelection, paths)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      onActivate(file)
    } else if (e.key === ' ' && showCheckbox) {
      e.preventDefault()
      // Bulk-toggle the visual multi-selection when one exists and the
      // focused row is part of it; otherwise fall back to single toggle.
      // Match the master-checkbox rule: any-excluded → include all, else
      // exclude all.
      if (rowSelection.keys.size > 1 && rowSelection.keys.has(file.path) && onBulkToggle) {
        const selected = [...rowSelection.keys]
        const anyExcluded = selected.some((p) => !selectedFiles.has(p))
        onBulkToggle(selected, anyExcluded)
      } else {
        onToggle(file)
      }
    }
  }

  function handleCheckboxClick(e: MouseEvent, file: FileEntry) {
    e.stopPropagation()
    // The checkbox is `disabled` for dirty submodules so this shouldn't fire,
    // but guard anyway: it can never be staged from the parent repo.
    if (file.submodule_dirty) {
      e.preventDefault()
      return
    }
    if (e.shiftKey && checkboxAnchor && checkboxAnchor !== file.path) {
      // Cancel the native toggle — we'll drive every checkbox in the range,
      // including this one, via the bulk callback.
      e.preventDefault()
      const willInclude = !selectedFiles.has(file.path)
      onBulkToggle?.(rangeBetween(paths, checkboxAnchor, file.path), willInclude)
      checkboxAnchor = file.path
      return
    }
    // Plain click: native toggle fires; we just record the anchor + propagate.
    checkboxAnchor = file.path
    onToggle(file)
  }

  // Keep the highlight describing files that still exist, after a commit /
  // discard / external `git rm` takes some out of the working tree — and when
  // nothing the user highlighted is left, re-seat it on the file the owner has
  // open. That second half is also what gives the file the owner opened by
  // itself (the first one, on arrival) a selection to extend *from*, so a
  // shift-click straight after arriving has an anchor.
  //
  // Keyed on the path list and the open file, not on the selection — gestures
  // keep it valid themselves, and reading it here would re-run this on every
  // click.
  $effect(() => {
    const order = paths
    const open = activeFile?.path ?? null
    untrack(() => {
      rowSelection = reseated(rowSelection, order, open)
      if (checkboxAnchor && !order.includes(checkboxAnchor)) checkboxAnchor = null
    })
  })

  let masterCheckbox = $state<HTMLInputElement | null>(null)

  // A dirty submodule (changed inside, pointer unmoved) can't be staged from
  // the parent repo, so it's never selectable — the master toggle and its
  // checked/indeterminate state ignore it, or it could never read "all checked".
  const selectable = (f: FileEntry) => !f.submodule_dirty
  const selectableFiles = $derived(files.filter(selectable))

  const allSelected = $derived(
    selectableFiles.length > 0 && selectableFiles.every((f) => selectedFiles.has(f.path)),
  )
  const isIndeterminate = $derived(
    !allSelected && selectableFiles.some((f) => selectedFiles.has(f.path)),
  )

  // What the header says, in the native client's words: how many of the files
  // that *can* be committed are going into this commit. "12 changed files"
  // restated the tab's own pill and answered a question the list already
  // answers by being visible, while the number the user is about to act on —
  // how many are checked — was nowhere. A dirty submodule is out of both
  // counts, since it can never be staged from here.
  const includedCount = $derived(selectableFiles.filter((f) => selectedFiles.has(f.path)).length)

  // `indeterminate` is a DOM property, not an attribute — Svelte can't render
  // it from markup. Reflect it via an effect whenever the derived flag flips.
  $effect(() => {
    if (masterCheckbox) masterCheckbox.indeterminate = isIndeterminate
  })

  function toggleAll(e?: Event) {
    e?.stopPropagation()
    onToggleAll?.(!allSelected)
  }

  // Colour is the one genuinely per-platform choice, so it stays here; the
  // letter and the name come from core (see `fileStatusStyles`) because the
  // two clients had already invented different ones for a conflict — the row
  // a user most needs to recognize.
  function getStatusColor(status: FileEntry['status']): string {
    switch (status) {
      case 'New':
        return 'var(--status-green)'
      // Orange, matching `FileStatus.tint` (`FileStatusStyle.swift:41`).
      // `--status-yellow` is an amber, and beside the green and red plates it
      // read as a fourth status rather than as the same one the native client
      // shows — Modified is the letter on almost every row, so it was the
      // single most-repeated colour disagreement between the two clients.
      case 'Modified':
        return 'var(--status-orange)'
      case 'Deleted':
        return 'var(--status-red)'
      case 'Renamed':
        return 'var(--status-blue)'
      // Deliberately not red: red already means Deleted, and a glance down the
      // list has to separate "you deleted this" from "git couldn't merge this"
      // — opposite actions, one of which blocks the commit.
      case 'Conflicted':
        return 'var(--status-purple)'
      default:
        return 'var(--text-secondary)'
    }
  }

  // ---- Context menu ----------------------------------------------------
  // Tracks the open menu's anchor and the files it acts on. Null when closed.
  let contextMenu = $state<{ x: number; y: number; files: FileEntry[] } | null>(null)

  function openContextMenu(e: MouseEvent, file: FileEntry) {
    // No actions wired (e.g. commit-detail list) → leave the native menu alone.
    if (!contextActions) return
    e.preventDefault()
    e.stopPropagation()
    // Right-clicking inside a multi-row selection acts on the whole selection;
    // right-clicking elsewhere re-selects just that row first (Finder / GH
    // Desktop semantics).
    const targetPaths = new Set(contextTargets(rowSelection, paths, file.path))
    if (targetPaths.size === 1) selectRow(file, 'replace')
    const targets = files.filter((f) => targetPaths.has(f.path))
    contextMenu = { x: e.clientX, y: e.clientY, files: targets }
  }

  // Build the menu for the current target set. Multi-selection collapses to the
  // bulk discard (the only action that's meaningful for many files at once);
  // a single file gets the full menu.
  const menuItems = $derived.by<ContextMenuItem[]>(() => {
    const actions = contextActions
    const menu = contextMenu
    if (!actions || !menu) return []
    const targets = menu.files

    if (targets.length > 1) {
      return [
        {
          label: `Discard ${targets.length} Selected Changes…`,
          destructive: true,
          action: () => actions.discard(targets),
        },
      ]
    }

    const file = targets[0]
    if (!file) return []
    const ext = fileExtension(file.path)
    const onDisk = file.status !== 'Deleted'

    const items: ContextMenuItem[] = [
      { label: 'Discard Changes…', destructive: true, action: () => actions.discard([file]) },
      MENU_SEPARATOR,
      { label: 'Ignore File (Add to .gitignore)', action: () => actions.ignoreFile(file) },
    ]
    if (ext) {
      items.push({
        label: `Ignore All ${ext} Files (Add to .gitignore)`,
        action: () => actions.ignoreExtension(ext),
      })
    }
    items.push(
      MENU_SEPARATOR,
      { label: 'Copy File Path', action: () => actions.copyPath(file) },
      { label: 'Copy Relative File Path', action: () => actions.copyRelativePath(file) },
      MENU_SEPARATOR,
      { label: revealLabel(), enabled: onDisk, action: () => actions.reveal(file) },
      {
        label: 'Open with Default Program',
        enabled: onDisk,
        action: () => actions.openWithDefault(file),
      },
    )
    return items
  })

  // Drop the menu if its target files leave the list (e.g. a background status
  // refresh removes a file the menu was anchored to).
  $effect(() => {
    if (!contextMenu) return
    const present = new Set(files.map((f) => f.path))
    if (!contextMenu.files.every((f) => present.has(f.path))) contextMenu = null
  })

</script>

<div class="file-list" style="--row-height: {ROW_HEIGHT}px">
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
        {includedCount} of {selectableFiles.length} files included
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
          {@const isRowSelected = rowSelection.keys.has(file.path)}
          <div
            class="file-row virtual-row"
            class:active={isActive}
            class:included={isSelected}
            class:row-selected={isRowSelected}
            class:striped={fileIndex % 2 === 1}
            class:submodule-dirty={file.submodule_dirty}
            data-file-row-index={fileIndex}
            style="top: {fileIndex * ROW_HEIGHT}px;"
            onclick={(e) => selectRow(file, clickGesture(e))}
            oncontextmenu={(e) => openContextMenu(e, file)}
            onkeydown={(e) => handleRowKeyDown(e, file, fileIndex)}
            role="button"
            tabindex="0"
          >
            {#if showCheckbox}
              <input
                type="checkbox"
                class="file-checkbox"
                checked={isSelected}
                disabled={file.submodule_dirty}
                title={file.submodule_dirty ? SUBMODULE_DIRTY_HINT : undefined}
                aria-label={file.submodule_dirty
                  ? SUBMODULE_DIRTY_HINT
                  : isSelected
                    ? `Exclude ${file.path} from commit`
                    : `Include ${file.path} in commit`}
                onclick={(e) => handleCheckboxClick(e, file)}
                onkeydown={(e) => e.stopPropagation()}
              />
            {/if}

            {#if file.embedded}
              <div
                class="status-badge"
                style="--badge-tint: var(--status-blue)"
                title="Nested Git repository — commits as a link, not its files"
              >
                ↪
              </div>
            {:else if file.submodule_dirty}
              <div
                class="status-badge"
                style="--badge-tint: var(--text-muted)"
                title={SUBMODULE_DIRTY_HINT}
              >
                ↪
              </div>
            {:else}
              <div
                class="status-badge"
                style="--badge-tint: {getStatusColor(file.status)}"
                title={$fileStatusStyles[file.status]?.label ?? file.status}
              >
                {$fileStatusStyles[file.status]?.letter ?? file.status.charAt(0)}
              </div>
            {/if}

            {#if file.orig_path}
              <div class="file-info">
                <PathText path={file.orig_path} dim />
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

<!--
  Rendered outside `.file-list` so the menu's `position: fixed` resolves against
  the viewport. (FileList has no `transform` ancestor, but keeping it a sibling
  matches CommitList and is robust if that changes.)
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
  /* No padding above the header: `ChangesSidebar.swift`'s pane is a
     `VStack(spacing: 0)` whose first child is the header itself, and the
     header's own `.padding(.vertical, 6)` is the whole of the gap between the
     tab bar and the checkbox. The checkbox-less variant starts at the split's
     edge for the same reason (`HistoryDetailPane.swift:105`). */
  .file-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-secondary);
    min-height: 0;
  }

  /*
    The actual scroller. Rows are absolutely positioned inside .rows-spacer,
    which carries the full virtual height so the scrollbar tracks the true
    document size.

    Its padding is the list's own inset, and it is symmetric on purpose: the
    first row must not sit flush against the divider above it any more than the
    last one may against the composer below. The 6px on each side is half of
    the 12px the rows and the header both start their content at — see
    `.file-row`.
  */
  .rows-viewport {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    padding: 4px 6px;
    min-height: 0;
  }

  .rows-spacer {
    position: relative;
    width: 100%;
  }

  /* `EmptyListPlaceholder.swift` draws this line with
     `.foregroundStyle(.tertiary)`, and this client's stand-in for `.tertiary`
     is `--text-faint`: `STYLE.md`'s colour table names it for exactly this
     ("Empty-state hints"), and `CommitList.svelte` — the other caller of the
     same native placeholder — already uses it. `--text-muted` is the register
     for secondary *information* that is still being read (authors, dates,
     paths), which this line is not; it is a note saying there is nothing to
     read. */
  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    /* `height: 100%`, not `flex: 1`: the viewport this sits in is a block
       scroller, so a flex property here is inert and the box would be one
       line tall at the top of the list — which is where this line sat while
       `CommitList`'s, written this way, centred. The native placeholder
       claims the whole slot (`EmptyListPlaceholder.swift:15`). */
    height: 100%;
    color: var(--text-faint);
    font-size: 13px;
  }

  /*
    `ChangedFileList.swift`'s row: `HStack(spacing: 10) { leading; badge; path }`
    — hence the 10px gap. The select-all header below is a different stack and
    keeps its own 8px.
  */
  .file-row {
    display: flex;
    align-items: center;
    gap: 10px;
    /* 6 here on top of the scroller's own 6 puts a row's checkbox 12px from
       the pane's edge — exactly where the header above puts its select-all
       checkbox (`ChangesSidebar.swift` pads that stack by 12). The two
       checkboxes are the leading item of their respective stacks and read as
       one column, so they have to start on the same line; splitting the 12
       across the two boxes is what leaves the rounded selection inset from the
       pane rather than flush against it. */
    padding: 0 6px;
    /* Published by the wrapper from `ROW_HEIGHT`, which is also the step the
       virtualizer positions by — one number, so they cannot drift. */
    height: var(--row-height);
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

  /*
    Alternating row backgrounds, as the native list has
    (`ChangedFileList.swift:114`). It keys off the file's index in `files`,
    **never** off DOM position: rows are virtualized and absolutely
    positioned, so `:nth-child` sees only the handful near the viewport and
    would restripe the whole list on every scroll. Row 0 is the plain one.

    Two deliberate departures from `NSTableView`, both of which make this the
    better half of the pair:

    - **Only real rows are striped.** AppKit's hook paints the *clip rect*,
      so a two-file repository gets a column of empty plates running down to
      the composer — placeholder rows for files that do not exist. Striping
      the row element cannot do that.
    - **A lower alpha than the measured 4.7 % white.** The native list has no
      hover state for a stripe to collide with; this one does, at 6 %. Landing
      the stripe within 1.3 % of hover would have traded a state the user
      relies on for a texture they do not, so the stripe sits at roughly half
      the hover value and all four backgrounds stay distinguishable.

    Ordered before `:hover`, `.row-selected` and `.active` on purpose: all
    four weigh (0,2,0), so source order is the whole of the cascade here and a
    stripe declared later would paint over the row the pointer is on.
  */
  .file-row.striped {
    background: var(--surface-stripe);
  }

  .file-row:hover {
    background: var(--surface-hover);
  }

  /*
    Visual multi-row highlight — every row in the selection, however it got
    there (a shift range, a ⌘/Ctrl-click, ⌘A).
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
    The include-all header — `ChangesSidebar.swift`'s `listHeader`, which is
    `HStack(spacing: 8) { Toggle; Text(…).font(.caption).foregroundStyle(.secondary); Spacer }`
    with `.padding(.horizontal, 12).padding(.vertical, 6)`, followed by a
    `Divider()` inside a `VStack(spacing: 0)`.

    Its own gap and padding, because it is a different stack from the rows
    below it: 8 rather than the row's 10, and a height that falls out of its
    own padding rather than taking the rows' fixed pitch — it carries no 18px
    badge, so the number that sizes a row would only leave it slack.

    The rule under it is that `Divider()`, so it is full-bleed and flush
    against the header: it separates two regions of the pane, and a line inset
    from the edges and floated off the text above it reads instead as an
    underline belonging to that text.
  */
  .select-all-row {
    color: var(--text-secondary);
    gap: 8px;
    height: auto;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-inactive);
    border-radius: 0;
  }

  /* `.font(.caption)` at `ChangesSidebar.swift`, which on macOS — unlike
     iOS — is 10pt regular (`docs/plans/tauri-reskin.md` §10.2 P-19 reads the
     same pair off both clients). Smaller and lighter than the filenames it
     counts, which is what keeps it reading as a header rather than as the
     first row of the list. */
  .select-all-label {
    font-size: 10px;
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Size, cursor, accent and focus ring are the app-wide checkbox shape in
     `app.css`. What is left here is what only a row's checkbox needs: it must
     not be squeezed by a long filename beside it, and it is the one checkbox in
     the app that can be disabled while the list is busy. */
  .file-checkbox {
    flex-shrink: 0;
  }

  .file-checkbox:disabled {
    cursor: not-allowed;
    opacity: 0.4;
  }

  /*
    The status letter on a tinted plate of its own colour — the native client's
    treatment, adopted here so the two clients read alike. The plate is what
    makes the letter a badge at a glance rather than a stray glyph in a column
    of filenames; `--badge-tint` is set per row so the letter and its wash can
    never disagree about which status they are showing.
  */
  .status-badge {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    color: var(--badge-tint, var(--text-secondary));
    background: color-mix(in srgb, var(--badge-tint, var(--text-secondary)) 15%, transparent);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  /*
    A rename's two sides and the arrow between them — `ChangedFileList.swift`'s
    `pathLabel`, which is an `HStack(spacing: 4)`. Centre-aligned, not
    baseline-aligned, because that HStack takes SwiftUI's default `.center`;
    the two sides are the same face at the same size, so the two agree today
    and would diverge the moment one side's weight moved (an included row
    raises its filename to medium).
  */
  .file-info {
    display: flex;
    align-items: center;
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    white-space: nowrap;
    gap: 4px;
  }

  /* `.foregroundStyle(.secondary)` on the native arrow, and `.fixedSize()` so
     it never gives up width to the two greedy paths it separates. */
  .arrow {
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .file-row.included :global(.filename) {
    font-weight: 500;
  }

  /* Dirty submodule: can't be staged from the parent, so the row reads as
     inactive while still being clickable to view its diff. Native passes
     `isMuted: file.submoduleDirty` into `PathText`, which resolves the
     filename to `.secondary` — the same treatment a rename's "from" side
     gets, and the same level as the directory already beside it. */
  .file-row.submodule-dirty :global(.filename) {
    color: var(--text-secondary);
  }
</style>

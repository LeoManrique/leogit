/**
 * Keyboard focus for the hand-rolled virtual lists (the changed-file list and
 * the commit list). Both mount only the rows near the viewport, so "focus row
 * N" is three steps in a fixed order, and the order is the part worth sharing.
 */

import { tick } from 'svelte'

interface FocusVirtualRowOptions {
  /** The scrolling element the rows are positioned inside. */
  container: HTMLElement | null | undefined
  /** The row's index in the model, not in the mounted slice. */
  index: number
  /** The list's fixed row pitch, in px. */
  rowHeight: number
  /** Selector for the mounted row, e.g. `[data-file-row-index="12"]`. */
  rowSelector: string
  /**
   * Told the new `scrollTop` the moment it is applied. The list must copy it
   * into the state its visible range derives from *synchronously*: the scroll
   * event arrives later, and waiting for it would focus before the row exists.
   */
  onScroll: (scrollTop: number) => void
}

/**
 * Bring a row into view, wait for it to be mounted, then focus it — so the next
 * arrow press continues from there. A row already in view does not move.
 */
export async function focusVirtualRow(options: FocusVirtualRowOptions): Promise<void> {
  const { container, index, rowHeight, rowSelector, onScroll } = options
  if (!container) return

  const top = index * rowHeight
  const bottom = top + rowHeight
  let scrollTop = container.scrollTop
  if (top < scrollTop) scrollTop = top
  else if (bottom > scrollTop + container.clientHeight) scrollTop = bottom - container.clientHeight
  if (scrollTop !== container.scrollTop) {
    container.scrollTop = scrollTop
    onScroll(scrollTop)
  }

  await tick()
  container.querySelector<HTMLElement>(rowSelector)?.focus({ preventScroll: true })
}

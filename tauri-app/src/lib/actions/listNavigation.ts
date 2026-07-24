/**
 * Keyboard navigation shared by the searchable repo lists (the startup picker,
 * the header switcher, and the Clone dialog). Each list keeps its own
 * `activeIndex` state; these helpers provide the two pieces that would
 * otherwise be copy-pasted into every list: computing the next highlight for an
 * arrow key, and scrolling the highlighted row into view.
 */

/** Direction of an arrow-key move over a list: Down (1) or Up (-1). */
export type NavDelta = 1 | -1

/**
 * Next highlight index for an Up/Down arrow over a list of `count` items.
 * Wraps at both ends, and treats "no selection" (-1) as sitting just before the
 * first row so Down → first and Up → last. Returns -1 for an empty list.
 */
export function nextActiveIndex(current: number, count: number, delta: NavDelta): number {
  if (count <= 0) return -1
  if (current < 0) return delta === 1 ? 0 : count - 1
  return (current + delta + count) % count
}

/**
 * Svelte action: scroll the row into its scroll container whenever it becomes
 * the active (keyboard-highlighted) row. `block: 'nearest'` means an
 * already-visible row never jumps, so mouse users see no motion.
 */
export function scrollIntoViewWhenActive(node: HTMLElement, active: boolean) {
  if (active) node.scrollIntoView({ block: 'nearest' })
  return {
    update(next: boolean) {
      if (next) node.scrollIntoView({ block: 'nearest' })
    },
  }
}

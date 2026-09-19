/**
 * The selection rules every multi-row list in this client follows — the
 * changed-file list and the commit list today. One module so the two cannot
 * answer the same gesture differently; each list keeps its own rows, paging and
 * menus, and hands the *rules* to this.
 *
 * A selection is a **set of row keys plus an anchor**. Rows are addressed by a
 * stable key (a path, a sha) rather than by index, so a reload that replaces
 * every row value keeps the highlight. Everything here is a pure function of
 * the selection and of `order` — the keys of the list as it is drawn — and
 * returns the same object when nothing changed, so a caller can publish the
 * result without waking its subscribers for a no-op.
 *
 * The native client gets the gestures from AppKit and shares only the last two
 * rules with this file (`ListSelection.swift`): which single row a detail pane
 * shows, and how a selection survives a reload.
 */

import { platformModifierHeld } from './platform'

export interface ListSelection {
  /** The highlighted rows, by key. */
  readonly keys: ReadonlySet<string>
  /**
   * The row a shift gesture extends from. Always one of `keys`, and `null`
   * only while nothing is selected.
   */
  readonly anchor: string | null
}

export const EMPTY_SELECTION: ListSelection = { keys: new Set(), anchor: null }

/**
 * What a click or a key press asks of the selection: make this row the whole
 * of it, stretch it from the anchor to this row, or flip this one row.
 */
export type SelectionGesture = 'replace' | 'extend' | 'toggle'

/**
 * The gesture a click carries. The toggle modifier is the platform's own — ⌘ on
 * macOS, Ctrl elsewhere — and it is read before Shift, as Finder and Explorer
 * both do. Ctrl-click on macOS never reaches this: it is a right-click there.
 */
export function clickGesture(e: MouseEvent): SelectionGesture {
  if (platformModifierHeld(e)) return 'toggle'
  return e.shiftKey ? 'extend' : 'replace'
}

/** The gesture a navigation key carries: Shift extends, anything else moves. */
export function keyGesture(e: KeyboardEvent): SelectionGesture {
  return e.shiftKey ? 'extend' : 'replace'
}

/**
 * The row a navigation key moves to from `index`, or `null` when the key is
 * not one. Home/End are tested first: on macOS they arrive as ⌘↑ / ⌘↓, so they
 * have to beat the plain arrows. The result is clamped to the list.
 */
export function rowIndexForKey(e: KeyboardEvent, index: number, count: number): number | null {
  if (count === 0) return null
  let target: number
  if (e.key === 'Home' || (e.key === 'ArrowUp' && e.metaKey)) target = 0
  else if (e.key === 'End' || (e.key === 'ArrowDown' && e.metaKey)) target = count - 1
  else if (e.key === 'ArrowDown') target = index + 1
  else if (e.key === 'ArrowUp') target = index - 1
  else return null
  return Math.max(0, Math.min(count - 1, target))
}

/** Whether this is the platform's Select All chord (⌘A, or Ctrl+A). */
export function isSelectAllChord(e: KeyboardEvent): boolean {
  return platformModifierHeld(e) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'a'
}

/** One row, which is also where the next shift gesture starts. */
function selectOnly(key: string): ListSelection {
  return { keys: new Set([key]), anchor: key }
}

/**
 * Every row. The anchor stays where it was when it can, so a shift-click after
 * ⌘A still extends from the row the user last chose.
 */
export function selectAll(selection: ListSelection, order: readonly string[]): ListSelection {
  if (order.length === 0) return EMPTY_SELECTION
  const anchor =
    selection.anchor !== null && order.includes(selection.anchor) ? selection.anchor : order[0]
  return { keys: new Set(order), anchor }
}

/** The keys from `from` to `to` inclusive, in list order; empty if either is gone. */
export function rangeBetween(order: readonly string[], from: string, to: string): string[] {
  const a = order.indexOf(from)
  const b = order.indexOf(to)
  if (a < 0 || b < 0) return []
  return a <= b ? order.slice(a, b + 1) : order.slice(b, a + 1)
}

/**
 * The selection after `gesture` lands on the row `key`.
 *
 * - **replace** — that row alone, and the anchor moves to it.
 * - **extend** — the range from the anchor to that row *replaces* the
 *   selection, and the anchor stays put, so the user can keep re-extending
 *   from the same spot (Finder's rule). With no anchor to extend from it is a
 *   replace.
 * - **toggle** — flips that one row. The last selected row cannot be toggled
 *   off: a list whose detail pane always shows something keeps something
 *   selected. Toggling a row on moves the anchor to it; toggling the anchor
 *   off hands the anchor to the first row still selected.
 */
export function applyGesture(
  selection: ListSelection,
  order: readonly string[],
  key: string,
  gesture: SelectionGesture
): ListSelection {
  if (gesture === 'extend' && selection.anchor !== null) {
    const range = rangeBetween(order, selection.anchor, key)
    if (range.length > 0) {
      // Shift+↓ held at the last row asks for the range it already has.
      const unchanged =
        range.length === selection.keys.size && range.every((k) => selection.keys.has(k))
      return unchanged ? selection : { keys: new Set(range), anchor: selection.anchor }
    }
  }

  if (gesture === 'toggle') {
    if (!selection.keys.has(key)) {
      return { keys: new Set([...selection.keys, key]), anchor: key }
    }
    if (selection.keys.size === 1) return selection
    const keys = new Set(selection.keys)
    keys.delete(key)
    const anchor = selection.anchor === key ? firstInOrder(keys, order) : selection.anchor
    return { keys, anchor }
  }

  const alreadyAlone =
    selection.keys.size === 1 && selection.keys.has(key) && selection.anchor === key
  return alreadyAlone ? selection : selectOnly(key)
}

/**
 * Keep the selection describing rows that still exist.
 *
 * Two things, in order. Rows that have left the list are pruned, because a
 * highlight pointing at nothing would still be counted by every action that
 * reads it. Then, and only if nothing is left, the selection re-seats on
 * `fallback` — the row the owner wants selected when the user's choice is gone
 * (the row its pane is showing, the newest commit). Those are the two
 * conditions and no others: a row the user chose is never overridden.
 */
export function reseated(
  selection: ListSelection,
  order: readonly string[],
  fallback: string | null
): ListSelection {
  const present = new Set(order)
  const survivors = [...selection.keys].filter((k) => present.has(k))
  if (survivors.length === 0) {
    if (fallback === null || !present.has(fallback)) {
      return selection.keys.size === 0 ? selection : EMPTY_SELECTION
    }
    return selectOnly(fallback)
  }

  const anchorAlive = selection.anchor !== null && present.has(selection.anchor)
  if (survivors.length === selection.keys.size && anchorAlive) return selection
  const keys = new Set(survivors)
  return { keys, anchor: anchorAlive ? selection.anchor : firstInOrder(keys, order) }
}

/**
 * What a right-click on the row `key` acts on: the whole selection, in list
 * order, when it lands inside a multi-row one — otherwise that row alone, which
 * the caller then selects, so a menu can never act on a row the user is not
 * looking at (Finder's rule, and `contextMenu(forSelectionType:)`'s).
 *
 * List order, never set order: a menu that names a count, or a range of
 * history, has to describe the rows as they are drawn.
 */
export function contextTargets(
  selection: ListSelection,
  order: readonly string[],
  key: string
): string[] {
  if (selection.keys.size > 1 && selection.keys.has(key)) {
    return order.filter((k) => selection.keys.has(k))
  }
  return [key]
}

/**
 * Which single row a detail pane shows for this selection — the rule the
 * native client calls `ListSelection.activeID`.
 *
 * One row selected: that row. Several: the one already showing when it is
 * among them, otherwise the first in list order. None: whatever was showing,
 * while it still exists. This client's lists move the pane to the *clicked*
 * row themselves, since their gestures know which row that was; this answers
 * the cases with no clicked row — a row toggled off, ⌘A, a reload.
 */
export function activeKey(
  selection: ListSelection,
  order: readonly string[],
  current: string | null
): string | null {
  const live = current !== null && order.includes(current) ? current : null
  if (selection.keys.size === 0) return live
  if (live !== null && selection.keys.has(live)) return live
  return firstInOrder(selection.keys, order)
}

function firstInOrder(keys: ReadonlySet<string>, order: readonly string[]): string | null {
  return order.find((k) => keys.has(k)) ?? null
}

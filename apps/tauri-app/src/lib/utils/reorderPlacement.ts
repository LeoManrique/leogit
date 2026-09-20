// Where a reorder's insertion line may sit in the History list, and what each
// place means to core. Pure, and mirrored natively by
// `Services/ReorderPlacement.swift`.
//
// A *slot* is a gap between rows: slot `i` is the one above row `i`, and slot
// `commits.length` is the one under the last loaded row. Moving commits to a
// slot reads as "take the moved rows out, and put them back where the line is".

import type { CommitInfo } from '$lib/api/commands'

/**
 * The slots that would leave the branch as it is, as a closed range: the ones
 * touching a contiguous selection, and inside it. Null for a selection with
 * gaps — gathering it anywhere is a change — and for one that is not in the
 * list at all.
 */
function unchangedSlots(
  commits: readonly CommitInfo[],
  moving: ReadonlySet<string>
): { first: number; last: number } | null {
  const rows = commits.flatMap((commit, index) => (moving.has(commit.sha) ? [index] : []))
  if (rows.length === 0) return null
  const first = rows[0]
  const last = rows[rows.length - 1]
  return last - first + 1 === rows.length ? { first, last: last + 1 } : null
}

/** Whether putting `moving` into `slot` would leave the branch as it is. */
export function reorderChangesNothing(
  commits: readonly CommitInfo[],
  moving: ReadonlySet<string>,
  slot: number
): boolean {
  const unchanged = unchangedSlots(commits, moving)
  return unchanged !== null && slot >= unchanged.first && slot <= unchanged.last
}

/**
 * The slot the line appears in when the mode arms: the one above the topmost
 * moved row — where the commits are now, so that a ⏎ pressed at once asks for
 * as little as it can.
 */
export function homeReorderSlot(
  commits: readonly CommitInfo[],
  moving: ReadonlySet<string>
): number {
  return Math.max(
    0,
    commits.findIndex((commit) => moving.has(commit.sha))
  )
}

/**
 * The slots the line may rest in, top to bottom, for moving `moving`.
 *
 * - **No slot under a merge commit.** Core replays the branch from the older of
 *   the moved commits and the destination, and git cannot replay a merge from a
 *   flat todo — so the line stops above the newest merge. (The menu item is
 *   already off when a merge sits at or above a moved commit —
 *   `replaysMergeCommit`.)
 * - **No slot that would change nothing, except home**, so that every step of
 *   the line is a different move and ↓ from home is "one row down" however
 *   many rows are moving.
 *
 * `commits` is the History list, newest first and append-only from HEAD. The
 * slot under the last loaded row is a real destination even when older history
 * exists — core takes any commit of the branch as one — and paging only ever
 * adds slots below it.
 */
export function reorderSlots(
  commits: readonly CommitInfo[],
  moving: ReadonlySet<string>
): number[] {
  if (!commits.some((commit) => moving.has(commit.sha))) return []
  const newestMerge = commits.findIndex((commit) => commit.parents.length > 1)
  const lowest = newestMerge === -1 ? commits.length : newestMerge
  const home = homeReorderSlot(commits, moving)
  const unchanged = unchangedSlots(commits, moving)

  const slots: number[] = []
  for (let slot = 0; slot <= lowest; slot++) {
    const changesNothing = unchanged !== null && slot >= unchanged.first && slot <= unchanged.last
    if (slot === home || !changesNothing) slots.push(slot)
  }
  return slots
}

/** Whether `moving` has anywhere to go: a slot other than the one it is in. */
export function canReorder(commits: readonly CommitInfo[], moving: ReadonlySet<string>): boolean {
  return reorderSlots(commits, moving).length > 1
}

/** The next slot up (`-1`) or down (`1`) from `slot`; `slot` itself at an end. */
export function adjacentReorderSlot(slots: readonly number[], slot: number, step: -1 | 1): number {
  const at = slots.indexOf(slot)
  return at === -1 ? slot : (slots[at + step] ?? slot)
}

/**
 * A slot as core names a destination: the commit the moved block lands just
 * *under* — the row above the line — or null for the top slot, the tip.
 */
export function reorderDestination(commits: readonly CommitInfo[], slot: number): string | null {
  return slot === 0 ? null : (commits[slot - 1]?.sha ?? null)
}

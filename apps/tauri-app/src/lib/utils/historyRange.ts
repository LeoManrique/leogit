// What a rewrite of the current branch would replay, read off the loaded
// History rows. Pure, and mirrored natively by `Services/HistoryRange.swift`.

import type { CommitInfo } from '$lib/api/commands'

/**
 * Whether rewriting `selected` would replay a merge commit: one sits anywhere
 * from HEAD down to the oldest selected commit. A squash or a reorder replays
 * that whole stretch, and git cannot replay a merge from a flat todo.
 *
 * `commits` is the History list — newest first, append-only from HEAD — so
 * every row above a selected one is loaded and the answer needs no git call.
 * It is the menu's early answer only; core's preflight asks git and stays the
 * authority.
 */
export function replaysMergeCommit(
  commits: readonly CommitInfo[],
  selected: ReadonlySet<string>
): boolean {
  let oldest = -1
  commits.forEach((commit, index) => {
    if (selected.has(commit.sha)) oldest = index
  })
  return commits.slice(0, oldest + 1).some((commit) => commit.parents.length > 1)
}

// The way back from the last History action: what the strip says, and when it
// stops offering. Pure, and mirrored natively by `Services/UndoOffer.swift`.

import type { UndoPoint } from '$lib/api/commands'

/** The History actions that can be taken back. */
export type UndoableAction = 'cherryPick' | 'squash' | 'reorder'

/**
 * An Undo on offer. It lives in client memory only: after a restart the
 * branch's reflog is the way back.
 */
export interface UndoOffer {
  /** The repository the action ran in — never whichever one is open by now. */
  repoPath: string
  /** What the action did, as the strip states it. */
  sentence: string
  /** What core needs to put the branch back. */
  point: UndoPoint
  /**
   * The commits the action was asked for, newest first: what History selects
   * again once they are back.
   */
  restores: string[]
}

/**
 * What the strip says an action did. `count` is the number of commits it was
 * asked for, and `target` the branch a cherry-pick copied them onto.
 */
export function landedSentence(action: UndoableAction, count: number, target?: string): string {
  const commits = count === 1 ? '1 commit' : `${count} commits`
  switch (action) {
    case 'cherryPick':
      return `Cherry-picked ${commits} onto “${target ?? ''}”.`
    case 'squash':
      return `Squashed ${commits} into one.`
    case 'reorder':
      return `Reordered ${commits}.`
  }
}

/**
 * Whether the offer still stands under `status`. It falls once the status shows
 * the branch checked out at any commit but the one the action left it on — a
 * commit, an amend, a pull or another rewrite has moved it, here or in a
 * terminal — because an Undo that can only be refused is not left on screen to
 * fail. While another branch is checked out, or HEAD is detached, the status
 * cannot see that branch's tip: the offer stays, and core is the judge.
 */
export function undoStillStands(
  point: UndoPoint,
  status: { branch: string; detached: boolean; headSha: string }
): boolean {
  if (status.detached || status.branch !== point.branch) return true
  return status.headSha === point.after_sha
}

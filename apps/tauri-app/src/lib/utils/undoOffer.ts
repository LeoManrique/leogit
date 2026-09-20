// The way back from the last History action: what the strip says, when it
// stops offering, and what is remembered of an action that stopped on a
// conflict. Pure, and mirrored natively by `Services/UndoOffer.swift`.

import type { OperationInProgress, UndoPoint, UndoStart } from '$lib/api/commands'

/** A History action that can be taken back, with what its sentence names. */
export type UndoableAction =
  { kind: 'cherryPick'; target: string } | { kind: 'squash' } | { kind: 'reorder' }

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
 * asked for — what an Undo brings back, whatever a Continue made of them.
 */
export function landedSentence(action: UndoableAction, count: number): string {
  const commits = count === 1 ? '1 commit' : `${count} commits`
  switch (action.kind) {
    case 'cherryPick':
      return `Cherry-picked ${commits} onto “${action.target}”.`
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

/**
 * A History action that stopped on a conflict and left its operation open:
 * what is kept until the operation ends, so that a Continue which lands can
 * still be taken back and an Abort knows which branch to return to. Written
 * only by the app's own action, so an operation begun in a terminal has none.
 */
export interface OpenAction {
  /** The repository the action ran in. */
  repoPath: string
  /** What the action left open: a cherry-pick, or a squash's or reorder's rebase. */
  operation: OperationInProgress
  /** Where the action began, from core. */
  start: UndoStart
  action: UndoableAction
  /** The commits the action was asked for, newest first. */
  asked: string[]
}

/**
 * Whether `open` is still the operation `status` shows. It is forgotten the
 * moment the status shows anything else — aborted, finished or quit, here or
 * in a terminal. A cherry-pick is also held to its branch; a rebase cannot be,
 * because it detaches HEAD and the status names no branch while one is open.
 */
export function openActionStillStands(
  open: OpenAction,
  status: { operation: OperationInProgress | null; branch: string }
): boolean {
  if (status.operation !== open.operation) return false
  return open.operation === 'Rebase' || status.branch === open.start.branch
}

/**
 * The undo point for `open`, now that a Continue has carried its operation to
 * the end and `status` has been read again — or null when none can be vouched
 * for:
 *
 * - the status does not show the action's branch checked out, so its new tip
 *   cannot be read;
 * - `startedAt`, where git's own state says the operation began, is not where
 *   this action began — the open operation was not the one it left — or could
 *   not be read at all, which vouches for nothing;
 * - the branch is where it began, so there is nothing to take back.
 *
 * A wrong `after_sha` would be harmless — core refuses a tip that is not the
 * branch's — but nothing in core relates `before_sha` to it, which is why the
 * checks here are about the start.
 */
export function undoPointAfterContinue(
  open: OpenAction,
  startedAt: string | null,
  status: { branch: string; detached: boolean; headSha: string }
): UndoPoint | null {
  const { start } = open
  if (status.detached || status.branch !== start.branch || !status.headSha) return null
  if (startedAt !== start.before_sha) return null
  if (status.headSha === start.before_sha) return null
  return { ...start, after_sha: status.headSha }
}

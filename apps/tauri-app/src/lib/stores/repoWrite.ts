import { get, writable } from 'svelte/store'

/**
 * Kind of user-initiated write to the open repository — anything that moves
 * HEAD, a ref, the index or the working tree through git.
 */
export type RepoWriteKind =
  | 'commit'
  | 'continue'
  | 'switch'
  | 'create'
  | 'delete'
  | 'merge'
  | 'abort'
  | 'cherryPick'
  | 'squash'
  | 'reorder'
  | 'checkout'
  | 'undoCommit'
  | 'discard'

/**
 * The repository write in flight, or null when idle.
 *
 * One slot for the whole window, because git has one `index.lock`: a second
 * write started while the first still runs either fails on that lock or lands
 * in the middle of it — an Abort confirmed while a long Continue is replaying
 * commits, a branch switch under a commit. It lives in a store so the composer
 * and the layout, which share no props for it, ask the same question; the kind
 * is kept so each surface can tell its own work (the dialog that holds itself
 * open under a busy label) from someone else's (the button that goes inert).
 *
 * Network transfers keep their own slot (`activeNetworkOp`): they are waited
 * on differently, carry progress, and pause the poll. Appending to
 * `.gitignore` is not a holder either — it is a file edit that takes no git
 * lock.
 */
export const activeRepoWrite = writable<RepoWriteKind | null>(null)

/**
 * Claim the slot, or refuse. A refusal is never reported as a success: the
 * caller returns without dismissing the surface that asked, so no dialog
 * closes as though the work had been done.
 */
export function beginRepoWrite(kind: RepoWriteKind): boolean {
  if (get(activeRepoWrite) !== null) return false
  activeRepoWrite.set(kind)
  return true
}

/**
 * Whether the slot is held by a write other than `mine` — what a confirmation
 * left open under someone else's write disables its button on. Its own write
 * is a different state: the dialog holds itself open under a busy label.
 */
export function isHeldByAnother(active: RepoWriteKind | null, mine: RepoWriteKind): boolean {
  return active !== null && active !== mine
}

/** What a surface says while `isHeldByAnother` — one wording, everywhere. */
export const REPO_BUSY_MESSAGE = 'Another operation is still running.'

/** Release the slot — from the `finally` of whoever claimed it. */
export function endRepoWrite(): void {
  activeRepoWrite.set(null)
}

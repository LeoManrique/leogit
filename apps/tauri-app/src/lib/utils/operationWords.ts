import type { OperationInProgress } from '$lib/api/commands'

/**
 * The words for an operation in progress, in the three forms the UI needs.
 *
 * One table rather than a `switch` at each site: the branch chip, the abort
 * item, its confirmation and the composer all name the same operation, and a
 * fifth site that forgot a variant would show a rebase as a merge. Mirrored
 * natively by `OperationWords.swift`.
 */
export interface OperationWords {
  /** Sentence form: "the rebase in progress". */
  noun: string
  /** Title Case, for buttons and menu items: "Abort Rebase…". */
  title: string
  /** What the repository is doing, for the branch chip: "· rebasing". */
  gerund: string
}

const WORDS: Record<OperationInProgress, OperationWords> = {
  Merge: { noun: 'merge', title: 'Merge', gerund: 'merging' },
  Rebase: { noun: 'rebase', title: 'Rebase', gerund: 'rebasing' },
  CherryPick: { noun: 'cherry-pick', title: 'Cherry-pick', gerund: 'cherry-picking' },
  Revert: { noun: 'revert', title: 'Revert', gerund: 'reverting' },
}

export function operationWords(operation: OperationInProgress): OperationWords {
  return WORDS[operation]
}

/**
 * What to say when a Continue dropped the stopped commit instead of making it
 * (`OperationOutcome.skipped`). Git is silent there, and "nothing new in
 * History" would otherwise read as the Continue having done nothing.
 */
export function skippedNotice(words: OperationWords): string {
  return `The resolution left nothing to commit, so the ${words.noun} skipped that commit.`
}

/**
 * Whether the operation replays commits that already have a message, so the
 * composer offers **Continue** in place of **Commit**. A merge is the
 * exception: concluding it *is* a commit, with a message the user writes.
 */
export function continuesFromComposer(operation: OperationInProgress | null): boolean {
  return operation !== null && operation !== 'Merge'
}

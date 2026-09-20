// Run with `pnpm test` — Node's own runner, which strips the types itself. The
// helper under test imports nothing but types, which is what lets it load here
// without the bundler's `$lib` alias.

import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  landedSentence,
  openActionStillStands,
  undoPointAfterContinue,
  undoStillStands,
  type OpenAction,
} from '../src/lib/utils/undoOffer.ts'

const point = {
  branch: 'main',
  before_sha: 'b'.repeat(40),
  after_sha: 'a'.repeat(40),
  return_branch: null,
}

/** A status, as the rule reads it. */
function on(branch: string, headSha: string, detached = false) {
  return { branch, headSha, detached }
}

test('the offer stands while the branch is where the action left it', () => {
  assert.equal(undoStillStands(point, on('main', point.after_sha)), true)
})

test('the offer falls once the branch is checked out anywhere else', () => {
  assert.equal(undoStillStands(point, on('main', 'c'.repeat(40))), false)
  // Back on the commit it started from — somebody reset it by hand.
  assert.equal(undoStillStands(point, on('main', point.before_sha)), false)
})

test('another branch says nothing about this one, so core stays the judge', () => {
  assert.equal(undoStillStands(point, on('topic', 'c'.repeat(40))), true)
})

test('a detached HEAD says nothing either, a rebase of the branch included', () => {
  assert.equal(undoStillStands(point, on('', 'c'.repeat(40), true)), true)
  // Core empties `branch` whenever HEAD is detached, so this status does not
  // occur today; the flag is the rule's own word for it, and it has to win.
  assert.equal(undoStillStands(point, on('main', 'c'.repeat(40), true)), true)
})

test('a status that has not been read yet retires nothing', () => {
  assert.equal(undoStillStands(point, on('', '')), true)
})

test('the sentence counts the commits the action was asked for', () => {
  assert.equal(landedSentence({ kind: 'squash' }, 3), 'Squashed 3 commits into one.')
  assert.equal(landedSentence({ kind: 'reorder' }, 1), 'Reordered 1 commit.')
  assert.equal(landedSentence({ kind: 'reorder' }, 2), 'Reordered 2 commits.')
  assert.equal(
    landedSentence({ kind: 'cherryPick', target: 'release' }, 1),
    'Cherry-picked 1 commit onto “release”.'
  )
})

// ---- An action that stopped on a conflict ------------------------------------

const BEGAN = 'b'.repeat(40)
const LANDED = 'a'.repeat(40)

/** A squash of `main` that stopped: its rebase is open and HEAD is detached. */
const squash: OpenAction = {
  repoPath: '/repo',
  operation: 'Rebase',
  start: { branch: 'main', before_sha: BEGAN, return_branch: null },
  action: { kind: 'squash' },
  asked: ['1'.repeat(40), '2'.repeat(40)],
}

/** A cherry-pick from `main` onto `release` that stopped, with `release` checked out. */
const pick: OpenAction = {
  repoPath: '/repo',
  operation: 'CherryPick',
  start: { branch: 'release', before_sha: BEGAN, return_branch: 'main' },
  action: { kind: 'cherryPick', target: 'release' },
  asked: ['1'.repeat(40)],
}

test('an open squash is remembered although its rebase names no branch', () => {
  // A rebase detaches HEAD, and core reports no branch for a detached HEAD.
  assert.equal(openActionStillStands(squash, { operation: 'Rebase', branch: '' }), true)
})

test('an open cherry-pick is remembered while it is open on its target', () => {
  assert.equal(openActionStillStands(pick, { operation: 'CherryPick', branch: 'release' }), true)
})

test('an action is still remembered when its Continue stops on another conflict', () => {
  // The status after such a Continue is the one from before it, as far as
  // this rule reads it.
  assert.equal(openActionStillStands(squash, { operation: 'Rebase', branch: '' }), true)
  assert.equal(openActionStillStands(pick, { operation: 'CherryPick', branch: 'release' }), true)
})

test('an action is forgotten once its operation has ended, however it ended', () => {
  assert.equal(openActionStillStands(squash, { operation: null, branch: 'main' }), false)
  assert.equal(openActionStillStands(pick, { operation: null, branch: 'release' }), false)
  // A status that has not been read yet shows no operation either.
  assert.equal(openActionStillStands(pick, { operation: null, branch: '' }), false)
})

test('an action is forgotten under an operation of another kind', () => {
  assert.equal(openActionStillStands(squash, { operation: 'CherryPick', branch: 'main' }), false)
  assert.equal(openActionStillStands(pick, { operation: 'Merge', branch: 'release' }), false)
})

test('a cherry-pick open on another branch is not the one that was begun', () => {
  assert.equal(openActionStillStands(pick, { operation: 'CherryPick', branch: 'main' }), false)
})

test('a Continue that lands completes the start with the tip the status shows', () => {
  assert.deepEqual(undoPointAfterContinue(squash, BEGAN, on('main', LANDED)), {
    branch: 'main',
    before_sha: BEGAN,
    after_sha: LANDED,
    return_branch: null,
  })
  // A pick of several commits has git's record of its start; it is left on the target.
  assert.deepEqual(undoPointAfterContinue(pick, BEGAN, on('release', LANDED)), {
    branch: 'release',
    before_sha: BEGAN,
    after_sha: LANDED,
    return_branch: 'main',
  })
})

test('there is no undo when where the operation began could not be read', () => {
  // Core answers for every operation, a pick of one commit included; without
  // an answer nothing says the operation was this action's.
  assert.equal(undoPointAfterContinue(pick, null, on('release', LANDED)), null)
  assert.equal(undoPointAfterContinue(squash, null, on('main', LANDED)), null)
})

test('there is no undo when git says the operation began somewhere else', () => {
  // Quit in a terminal and begun again: same kind, same branch, another start.
  const elsewhere = 'c'.repeat(40)
  assert.equal(undoPointAfterContinue(squash, elsewhere, on('main', LANDED)), null)
  assert.equal(undoPointAfterContinue(pick, elsewhere, on('release', LANDED)), null)
})

test('there is no undo when the status cannot show the branch’s new tip', () => {
  assert.equal(undoPointAfterContinue(squash, BEGAN, on('', LANDED, true)), null)
  assert.equal(undoPointAfterContinue(squash, BEGAN, on('topic', LANDED)), null)
  assert.equal(undoPointAfterContinue(squash, BEGAN, on('main', '')), null)
})

test('a pick is held to its target after the Continue as it was before it', () => {
  // Back on the source, or anywhere else: the status cannot show the target's tip.
  assert.equal(undoPointAfterContinue(pick, BEGAN, on('main', LANDED)), null)
  assert.equal(undoPointAfterContinue(pick, BEGAN, on('', LANDED, true)), null)
})

test('there is no undo when the branch ended where it began', () => {
  assert.equal(undoPointAfterContinue(squash, BEGAN, on('main', BEGAN)), null)
  // Every picked commit resolved to nothing and was skipped.
  assert.equal(undoPointAfterContinue(pick, BEGAN, on('release', BEGAN)), null)
})

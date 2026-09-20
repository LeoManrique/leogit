// Run with `pnpm test` — Node's own runner, which strips the types itself. The
// helper under test imports nothing but types, which is what lets it load here
// without the bundler's `$lib` alias.

import assert from 'node:assert/strict'
import { test } from 'node:test'

import { landedSentence, undoStillStands } from '../src/lib/utils/undoOffer.ts'

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
  assert.equal(landedSentence('squash', 3), 'Squashed 3 commits into one.')
  assert.equal(landedSentence('reorder', 1), 'Reordered 1 commit.')
  assert.equal(landedSentence('reorder', 2), 'Reordered 2 commits.')
  assert.equal(landedSentence('cherryPick', 1, 'release'), 'Cherry-picked 1 commit onto “release”.')
})

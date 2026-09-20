// Run with `pnpm test` — Node's own runner, which strips the types itself. The
// helpers under test import nothing but types, which is what lets them load
// here without the bundler's `$lib` alias.

import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  adjacentReorderSlot,
  canReorder,
  homeReorderSlot,
  reorderChangesNothing,
  reorderDestination,
  reorderSlots,
} from '../src/lib/utils/reorderPlacement.ts'
import { replaysMergeCommit } from '../src/lib/utils/historyRange.ts'

/** A History list, newest first: a name per row, `M…` for a merge commit. */
function list(...names: string[]) {
  return names.map((sha) => ({ sha, parents: sha.startsWith('M') ? ['a', 'b'] : ['a'] })) as never
}

const five = list('e', 'd', 'c', 'b', 'a')

test('a contiguous selection skips the slots that would change nothing, except home', () => {
  const moving = new Set(['d', 'c'])
  assert.equal(homeReorderSlot(five, moving), 1)
  assert.deepEqual(reorderSlots(five, moving), [0, 1, 4, 5])
  assert.ok(reorderChangesNothing(five, moving, 1))
  assert.ok(reorderChangesNothing(five, moving, 3))
  assert.ok(!reorderChangesNothing(five, moving, 4))
})

test('a selection with gaps can be gathered anywhere', () => {
  const moving = new Set(['e', 'c'])
  assert.deepEqual(reorderSlots(five, moving), [0, 1, 2, 3, 4, 5])
  assert.ok(!reorderChangesNothing(five, moving, 0))
})

test('the line stops above the newest merge commit', () => {
  const merged = list('e', 'd', 'M', 'b', 'a')
  assert.deepEqual(reorderSlots(merged, new Set(['e'])), [0, 2])
  assert.ok(!replaysMergeCommit(merged, new Set(['e', 'd'])))
  assert.ok(replaysMergeCommit(merged, new Set(['b'])))
})

test('commits with nowhere to go cannot be reordered', () => {
  assert.ok(!canReorder(list('a'), new Set(['a'])))
  assert.ok(!canReorder(list('e', 'd', 'M'), new Set(['e', 'd'])))
  assert.ok(canReorder(five, new Set(['a'])))
  assert.deepEqual(reorderSlots(five, new Set(['gone'])), [])
})

test('the arrows step through the slots and rest at either end', () => {
  const slots = [0, 1, 4, 5]
  assert.equal(adjacentReorderSlot(slots, 1, 1), 4)
  assert.equal(adjacentReorderSlot(slots, 4, -1), 1)
  assert.equal(adjacentReorderSlot(slots, 0, -1), 0)
  assert.equal(adjacentReorderSlot(slots, 5, 1), 5)
  assert.equal(adjacentReorderSlot(slots, 3, 1), 3)
})

test('a slot names the commit the block lands under, and the top one the tip', () => {
  assert.equal(reorderDestination(five, 0), null)
  assert.equal(reorderDestination(five, 1), 'e')
  assert.equal(reorderDestination(five, 5), 'a')
})

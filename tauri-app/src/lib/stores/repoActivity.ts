import { writable } from 'svelte/store'
import { gitApi } from '$lib/api/commands'

// Module-level cache of each repo's last-commit Unix timestamp (seconds), so
// the repo picker's "recently modified" sort doesn't re-shell into git every
// time it opens. 0 means "no commits / unreadable" (sorts oldest).
const cache = new Map<string, number>()
const inflight = new Map<string, Promise<number>>()

// Reactive store the dropdown subscribes to so the sort re-settles as
// timestamps stream in (same pattern as repoIdentifiers).
export const repoActivity = writable<Map<string, number>>(new Map())

function publish() {
  repoActivity.set(new Map(cache))
}

/**
 * Fetch last-commit timestamps for any uncached paths. Fire-and-forget —
 * results land in the `repoActivity` store as they arrive. Safe to call
 * repeatedly; in-flight requests aren't re-issued.
 */
export function ensureRepoActivity(paths: string[]): void {
  for (const path of paths) {
    if (cache.has(path) || inflight.has(path)) continue
    const p = gitApi
      .getLastCommitTimestamp(path)
      .then((ts) => {
        cache.set(path, ts)
        publish()
        return ts
      })
      .catch(() => {
        cache.set(path, 0)
        publish()
        return 0
      })
      .finally(() => {
        inflight.delete(path)
      })
    inflight.set(path, p)
  }
}

// Sort preference for the repo picker, remembered for the session (not the
// disk config) so toggling it sticks across dropdown opens. Defaults to
// alphabetical — the picker's long-standing order — so opening it never
// reshuffles unexpectedly; the clock button opts into recency.
export type RepoSortMode = 'recent' | 'name'
let sortMode: RepoSortMode = 'name'

export function getRepoSortMode(): RepoSortMode {
  return sortMode
}

export function setRepoSortMode(mode: RepoSortMode): void {
  sortMode = mode
}

import { writable } from 'svelte/store'
import { gitApi, type RepoIdentifier } from '$lib/api/commands'

// Module-level cache so the dropdown doesn't re-shell into git every time it
// opens. Values:
//   undefined → not fetched yet
//   null      → fetched, no parseable remote (use folder basename as label)
//   {owner, name} → parsed identifier
const cache = new Map<string, RepoIdentifier | null>()

/**
 * How many `git remote get-url` subprocesses may be in flight at once.
 *
 * Opening the switcher asks for every discovered repository's label, and asking
 * for all of them at once was the last unbounded fan-out in either client: on a
 * machine with fifty repositories that is fifty processes spawned in one turn,
 * competing for the disk with the status poll and whatever the user is actually
 * doing. A small pool costs nothing visible — rows still fill in from the top,
 * a few at a time — and puts a ceiling on the burst.
 */
const MAX_IN_FLIGHT = 4

/** Paths waiting for a worker, and the set that keeps them from being queued twice. */
const queue: string[] = []
const queued = new Set<string>()
let inFlight = 0

// Reactive store the dropdown subscribes to so re-renders pick up newly
// fetched identifiers without us having to plumb a prop or callback through.
export const repoIdentifiers = writable<Map<string, RepoIdentifier | null>>(new Map())

function publish() {
  // Hand out a fresh Map so Svelte's identity check fires.
  repoIdentifiers.set(new Map(cache))
}

/** Take paths off the queue one at a time until it runs dry. */
async function drain(): Promise<void> {
  try {
    for (;;) {
      const path = queue.shift()
      if (path === undefined) return
      try {
        cache.set(path, await gitApi.getRepoIdentifier(path))
      } catch {
        cache.set(path, null)
      }
      queued.delete(path)
      publish()
    }
  } finally {
    inFlight -= 1
  }
}

/**
 * Fetch identifiers for any paths that aren't cached yet. Fire-and-forget —
 * results land in the `repoIdentifiers` store as they arrive. Safe to call
 * repeatedly; queued and cached paths aren't re-issued.
 */
export function ensureRepoIdentifiers(paths: string[]): void {
  for (const path of paths) {
    if (cache.has(path) || queued.has(path)) continue
    queued.add(path)
    queue.push(path)
  }
  while (inFlight < MAX_IN_FLIGHT && queue.length > 0) {
    inFlight += 1
    void drain()
  }
}

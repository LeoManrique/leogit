import { writable } from 'svelte/store'
import { gitApi, type RepoIdentifier } from '$lib/api/commands'

// Module-level cache so the dropdown doesn't re-shell into git every time it
// opens. Values:
//   undefined → not fetched yet
//   null      → fetched, no parseable remote (use folder basename as label)
//   {owner, name} → parsed identifier
const cache = new Map<string, RepoIdentifier | null>()
const inflight = new Map<string, Promise<RepoIdentifier | null>>()

// Reactive store the dropdown subscribes to so re-renders pick up newly
// fetched identifiers without us having to plumb a prop or callback through.
export const repoIdentifiers = writable<Map<string, RepoIdentifier | null>>(new Map())

function publish() {
  // Hand out a fresh Map so Svelte's identity check fires.
  repoIdentifiers.set(new Map(cache))
}

/**
 * Fetch identifiers for any paths that aren't cached yet. Fire-and-forget —
 * results land in the `repoIdentifiers` store as they arrive. Safe to call
 * repeatedly; in-flight requests aren't re-issued.
 */
export function ensureRepoIdentifiers(paths: string[]): void {
  for (const path of paths) {
    if (cache.has(path) || inflight.has(path)) continue
    const p = gitApi
      .getRepoIdentifier(path)
      .then((id) => {
        cache.set(path, id)
        publish()
        return id
      })
      .catch(() => {
        cache.set(path, null)
        publish()
        return null
      })
      .finally(() => {
        inflight.delete(path)
      })
    inflight.set(path, p)
  }
}

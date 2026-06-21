import { writable } from 'svelte/store'
import { gitApi } from '$lib/api/commands'

// Per-repo ahead/behind counts powering the repo picker's pull/push badges.
// Populated two ways: the tiered background scheduler (`syncRepo`) refreshes
// many repos on a cadence, and the active repo's status poll pushes its own
// counts in via `setRepoSync` — so the open repo's badge stays live (every 2s)
// without spending an extra fetch on it.

export interface SyncCounts {
  ahead: number
  behind: number
  /** Repos with no remote can't be ahead/behind; the picker shows no badge. */
  hasRemote: boolean
}

const cache = new Map<string, SyncCounts>()
// Guards against overlapping syncs for the same repo (a slow fetch tick
// colliding with an on-switch sync, say).
const inflight = new Set<string>()

export const repoSync = writable<Map<string, SyncCounts>>(new Map())

function publish(): void {
  repoSync.set(new Map(cache))
}

function sameCounts(a: SyncCounts | undefined, b: SyncCounts): boolean {
  return !!a && a.ahead === b.ahead && a.behind === b.behind && a.hasRemote === b.hasRemote
}

/**
 * Record a repo's ahead/behind directly (e.g. the active repo, whose counts
 * we already computed in `get_status`). No-op when unchanged so subscribers
 * don't re-render on every 2s poll.
 */
export function setRepoSync(path: string, counts: SyncCounts): void {
  if (sameCounts(cache.get(path), counts)) return
  cache.set(path, counts)
  publish()
}

/**
 * Refresh one repo's ahead/behind via the backend. `doFetch` controls whether
 * it hits the network first (the scheduler fetches; cheap recomputes don't).
 * Fire-and-forget: failures leave the previous value in place. In-flight
 * requests for the same path are coalesced.
 */
export async function syncRepo(path: string, doFetch: boolean): Promise<void> {
  if (inflight.has(path)) return
  inflight.add(path)
  try {
    const s = await gitApi.repoSyncStatus(path, doFetch)
    setRepoSync(path, { ahead: s.ahead, behind: s.behind, hasRemote: s.has_remote })
  } catch {
    // Transient (offline, repo moved) — keep the last-known counts.
  } finally {
    inflight.delete(path)
  }
}

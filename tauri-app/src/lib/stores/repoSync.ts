import { writable } from 'svelte/store'
import { gitApi } from '$lib/api/commands'
import { shouldAttemptBackground, recordResult } from '$lib/services/connectivity'

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
  /** Uncommitted working-tree changes — mirrors "Changes tab non-empty".
   * For the active repo it IS `status.files.length > 0`, so the dot can
   * never disagree with the tab the user is looking at. */
  dirty: boolean
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
  return (
    !!a &&
    a.ahead === b.ahead &&
    a.behind === b.behind &&
    a.hasRemote === b.hasRemote &&
    a.dirty === b.dirty
  )
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
 *
 * `background` marks automatic, timer-driven syncs (vs. a user opening a repo):
 * a background fetch is skipped entirely while we're offline / backing off, so
 * we don't keep spawning fetches we know will fail. Either way, the fetch's
 * outcome is reported to the connectivity breaker so it can trip or recover.
 */
export async function syncRepo(path: string, doFetch: boolean, background = false): Promise<void> {
  if (inflight.has(path)) return
  // A background *fetch* is pointless while offline / in a backoff window —
  // but the dirty flag is computed locally, so downgrade to a fetch-less
  // recompute instead of bailing: the dot keeps tracking working trees that
  // are edited while the network is down.
  if (doFetch && background && !shouldAttemptBackground()) doFetch = false
  inflight.add(path)
  try {
    const s = await gitApi.repoSyncStatus(path, doFetch)
    setRepoSync(path, { ahead: s.ahead, behind: s.behind, hasRemote: s.has_remote, dirty: s.dirty })
    // Only a real network attempt (fetch requested, remote exists) is a
    // connectivity signal; a no-remote repo says nothing about the link.
    if (doFetch && s.has_remote) recordResult(s.fetched)
  } catch {
    // The command itself failed (repo moved, parse error) — not a reliable
    // network signal, so leave the breaker alone and keep last-known counts.
  } finally {
    inflight.delete(path)
  }
}

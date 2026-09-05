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

/**
 * When each repo's remote was last reached, so a *background* fetch can decline
 * to repeat one that just happened. The only writer is a fetch that actually
 * succeeded — an unreachable remote leaves no stamp and is retried at the next
 * opportunity, which is the whole reason this is separate from the breaker.
 */
const lastFetchedAt = new Map<string, number>()

/**
 * How long a successful fetch answers for.
 *
 * Deliberately shorter than the shortest cadence that would otherwise refresh a
 * badge (the top tier's 2 min), so nothing is ever staler than it already would
 * have been: this removes duplicate round trips, it never removes a refresh.
 * The duplicates are real — the top tier fetches four repos, and 30 s later an
 * alt-tab's `kickTopTier` fetches the same four; switching A → B → A refetches
 * A seconds after opening it.
 */
const FETCH_COOLDOWN_MS = 60_000

/** Record that `path`'s remote was reached just now. */
export function noteFetched(path: string): void {
  lastFetchedAt.set(path, Date.now())
}

/**
 * Whether `path` was fetched recently enough that a background fetch would be
 * spending a round trip on an answer it already has.
 *
 * Only background work may ask. A user-initiated fetch or pull is the user
 * telling us the answer might be wrong, and it always runs.
 */
export function fetchedRecently(path: string): boolean {
  const at = lastFetchedAt.get(path)
  return at !== undefined && Date.now() - at < FETCH_COOLDOWN_MS
}

/** Seconds since the last successful fetch, for a log line. */
function fetchAgeSeconds(path: string): number {
  const at = lastFetchedAt.get(path)
  return at === undefined ? Infinity : Math.round((Date.now() - at) / 1000)
}

/** One `[fetch]` line naming what was skipped and how fresh the answer is. */
export function logCooldownSkip(path: string, what: string): void {
  console.log(`[fetch] ${what} for ${path}: fetched ${fetchAgeSeconds(path)}s ago`)
}

/**
 * Claim the per-repo sync slot from outside `syncRepo`, answering false when
 * somebody already holds it.
 *
 * The active repository's silent fetch runs `git fetch` directly rather than
 * through `syncRepo`, so without this it was invisible to the one guard that
 * keeps two fetches out of the same `.git` — and the loser of that race hits a
 * ref lock, fails, and is charged to the connectivity breaker as if the network
 * were down (F15). Callers must release in a `finally`.
 */
export function claimSyncSlot(path: string): boolean {
  if (inflight.has(path)) return false
  inflight.add(path)
  return true
}

/** Release what `claimSyncSlot` took. */
export function releaseSyncSlot(path: string): void {
  inflight.delete(path)
}

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
 * while offline / backing off such a sync drops its fetch and recomputes
 * locally instead, so we don't spawn fetches we know will fail without also
 * blinding the dirty dot. A fetch that does run reports its outcome to the
 * connectivity breaker so it can trip or recover.
 *
 * The same downgrade — never a skip — is what a repo inside its fetch cooldown
 * gets. It is also what `syncOnSwitch` inherits by going through here, so
 * switching A → B → A no longer refetches A seconds after opening it.
 */
export async function syncRepo(path: string, doFetch: boolean, background = false): Promise<void> {
  if (inflight.has(path)) return
  // A background *fetch* is pointless while offline / in a backoff window —
  // but the dirty flag is computed locally, so downgrade to a fetch-less
  // recompute instead of bailing: the dot keeps tracking working trees that
  // are edited while the network is down.
  if (doFetch && background && !shouldAttemptBackground()) doFetch = false
  // Same downgrade for a remote we reached a moment ago, and for the same
  // reason: the local half of this call is what keeps the dirty dot honest,
  // and it costs no network. Not charged to the breaker either — nothing was
  // attempted, so there is nothing to report about the link.
  if (doFetch && fetchedRecently(path)) {
    logCooldownSkip(path, 'recompute without fetching')
    doFetch = false
  }
  inflight.add(path)
  try {
    const s = await gitApi.repoSyncStatus(path, doFetch)
    setRepoSync(path, { ahead: s.ahead, behind: s.behind, hasRemote: s.has_remote, dirty: s.dirty })
    // Only a real network attempt (fetch requested, remote exists) is a
    // connectivity signal; a no-remote repo says nothing about the link.
    if (doFetch && s.has_remote) recordResult(s.fetched)
    // `fetched` alone is not "the remote replied": the backend documents it as
    // true when no fetch was requested and when there was no remote to reach,
    // because nothing failed. Reading it bare stamped every repo of a
    // fetch-less sweep — the one the picker runs the moment it opens — and the
    // repo the user then opened had its own on-open fetch turned away for the
    // next minute. A stamp needs all three: we asked, there was somewhere to
    // ask, and the answer came back — the same three the breaker gets told
    // about above.
    if (doFetch && s.has_remote && s.fetched) noteFetched(path)
  } catch {
    // The command itself failed (repo moved, parse error) — not a reliable
    // network signal, so leave the breaker alone and keep last-known counts.
  } finally {
    inflight.delete(path)
  }
}

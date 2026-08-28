import { writable } from 'svelte/store'
import { gitApi, type RepoIdentifier } from '$lib/api/commands'
import { basename } from '$lib/utils/path'

/** The cache as a reader sees it: a path maps to its identifier, to `null` for
 *  a repository with no parseable remote, or to nothing at all while it is
 *  still being looked up. */
type Identifiers = Map<string, RepoIdentifier | null>

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

// The label rules live here, beside the cache they read, because both repo
// pickers show the same rows and the pair had already started to diverge —
// the startup picker was labelling rows with raw paths while the dropdown
// showed the remote's name.

/** The row's own label: the remote's repo name when known, else the folder's. */
export function repoLabel(path: string, ids: Identifiers): string {
  return ids.get(path)?.name ?? basename(path)
}

/** The row's owner-qualified label, for the tooltip and the search index. */
export function repoFullLabel(path: string, ids: Identifiers): string {
  const id = ids.get(path)
  return id ? `${id.owner}/${id.name}` : basename(path)
}

/**
 * Every label a user might reasonably type for this row, for `filter_repos`.
 * One entry when the two coincide, so a repository with no remote isn't
 * matched twice against the same string.
 */
export function repoSearchLabels(path: string, ids: Identifiers): string[] {
  const label = repoLabel(path, ids)
  const full = repoFullLabel(path, ids)
  return label === full ? [label] : [label, full]
}

/**
 * Labels more than one row would show — GH Desktop's `needsDisambiguation`.
 * A row in this set earns a muted `owner/` prefix, and only then: a repository
 * with no remote has no owner to disambiguate with.
 */
export function collidingRepoLabels(paths: string[], ids: Identifiers): Set<string> {
  const seen = new Set<string>()
  const colliding = new Set<string>()
  for (const path of paths) {
    const label = repoLabel(path, ids)
    if (seen.has(label)) colliding.add(label)
    seen.add(label)
  }
  return colliding
}

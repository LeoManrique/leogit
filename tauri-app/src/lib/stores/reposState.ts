import { writable } from 'svelte/store'
import { configApi, type ReposState } from '$lib/api/commands'

export type SortMode = 'recent' | 'name'

// Both sort toggles default to recency ("last modified") — what you usually
// want when scanning for a repo (or remote) you were just in. The choice is
// persisted to disk (repos-state.json) so it survives restarts.
export const repoSortMode = writable<SortMode>('recent')
export const cloneSortMode = writable<SortMode>('recent')

// Repo paths in most-recently-opened-first order. Drives the picker's tiered
// background sync (the more recently you used a repo, the more often we refresh
// its pull/push badge). Persisted to repos-state.json so the tiering survives
// restarts. Capped so the file can't grow without bound.
export const recentRepos = writable<string[]>([])
const MAX_RECENTS = 50

function isSortMode(v: unknown): v is SortMode {
  return v === 'recent' || v === 'name'
}

let hydrated = false

/**
 * Load the persisted repos-state once at startup and seed the reactive
 * sort-mode stores. Safe to call repeatedly; only the first call hits disk.
 */
export async function hydrateReposState(): Promise<void> {
  if (hydrated) return
  hydrated = true
  try {
    const state = await configApi.loadState()
    if (isSortMode(state.repo_sort_mode)) repoSortMode.set(state.repo_sort_mode)
    if (isSortMode(state.clone_sort_mode)) cloneSortMode.set(state.clone_sort_mode)
    if (Array.isArray(state.recent_repos)) recentRepos.set(state.recent_repos)
  } catch {}
}

// Serializes all repos-state writes. patchReposState is a non-atomic
// read-modify-write spanning two IPC calls (loadState then saveState), and the
// backend file I/O takes no lock — so without this, two concurrent patches
// (e.g. rapid repo switches firing last_opened_repo + recent_repos updates)
// could both read the same on-disk state and the later save would drop the
// earlier field. Chaining every patch through one promise tail guarantees each
// load+save runs to completion before the next patch reads.
let writeTail: Promise<void> = Promise.resolve()

/**
 * Read-modify-write the persisted repos state so updating one field (a sort
 * mode, last_opened_repo, last_clone_dir, recent_repos) never clobbers another.
 * The single, serialized writer for everything in repos-state.json. The returned
 * promise resolves once this specific patch has been written.
 */
export function patchReposState(patch: Partial<ReposState>): Promise<void> {
  // The .then callback swallows its own errors so writeTail never rejects —
  // a rejected tail would break serialization for every subsequent patch.
  writeTail = writeTail.then(async () => {
    try {
      const current = await configApi.loadState().catch(() => ({}) as ReposState)
      await configApi.saveState({ ...current, ...patch })
    } catch {}
  })
  return writeTail
}

export function setRepoSortMode(mode: SortMode): void {
  repoSortMode.set(mode)
  patchReposState({ repo_sort_mode: mode })
}

export function setCloneSortMode(mode: SortMode): void {
  cloneSortMode.set(mode)
  patchReposState({ clone_sort_mode: mode })
}

/**
 * Mark a repo as just-opened: move it to the front of the recents list (most
 * recent first), de-duplicating and capping length, then persist. Called on
 * every repo open/switch so the sync scheduler always tiers off fresh usage.
 * Returns the persistence promise so callers can await the write if they need to.
 */
export function recordRecentRepo(path: string): Promise<void> {
  if (!path) return Promise.resolve()
  let next: string[] = []
  recentRepos.update((list) => {
    next = [path, ...list.filter((p) => p !== path)].slice(0, MAX_RECENTS)
    return next
  })
  return patchReposState({ recent_repos: next })
}

import { writable } from 'svelte/store'
import { configApi, type ReposStatePatch } from '$lib/api/commands'

export type SortMode = 'recent' | 'name'

// Both sort toggles default to recency ("last modified") — what you usually
// want when scanning for a repo (or remote) you were just in. The choice is
// persisted to disk (repos-state.json) so it survives restarts.
export const repoSortMode = writable<SortMode>('recent')
export const cloneSortMode = writable<SortMode>('recent')

// Repo paths in most-recently-opened-first order. Drives the picker's tiered
// background sync (the more recently you used a repo, the more often we refresh
// its pull/push badge). The backend owns the list (move-to-front, de-dupe,
// cap); this store mirrors the copy it returns.
export const recentRepos = writable<string[]>([])

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

/**
 * Merge the given fields into the persisted repos state. The backend applies
 * the patch as one atomic read-modify-write under a lock, so a patch can never
 * clobber another writer's fields. Never rejects — persistence failures are
 * logged, not surfaced, since losing a remembered sort mode isn't actionable.
 */
export async function patchReposState(patch: ReposStatePatch): Promise<void> {
  try {
    await configApi.patchState(patch)
  } catch (error) {
    console.error('[repos-state] patch failed:', error)
  }
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
 * Mark a repo as just-opened. The backend moves it to the front of the
 * persisted MRU list (de-duplicating and capping) and returns the
 * authoritative list, which reseeds the store — so the sync scheduler always
 * tiers off fresh usage. Called on every repo open/switch; resolves once the
 * write has landed, and never rejects (failures are logged).
 */
export async function recordRecentRepo(path: string): Promise<void> {
  if (!path) return
  try {
    const state = await configApi.recordRecentRepo(path)
    if (Array.isArray(state.recent_repos)) recentRepos.set(state.recent_repos)
  } catch (error) {
    console.error('[repos-state] recording recent repo failed:', error)
  }
}

import { writable } from 'svelte/store'
import { configApi, type ReposState } from '$lib/api/commands'

export type SortMode = 'recent' | 'name'

// Both sort toggles default to recency ("last modified") — what you usually
// want when scanning for a repo (or remote) you were just in. The choice is
// persisted to disk (repos-state.json) so it survives restarts.
export const repoSortMode = writable<SortMode>('recent')
export const cloneSortMode = writable<SortMode>('recent')

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
  } catch {}
}

/**
 * Read-modify-write the persisted repos state so updating one field (a sort
 * mode, last_opened_repo, last_clone_dir) never clobbers another. The single
 * writer for everything in repos-state.json.
 */
export async function patchReposState(patch: Partial<ReposState>): Promise<void> {
  try {
    const current = await configApi.loadState().catch(() => ({}) as ReposState)
    await configApi.saveState({ ...current, ...patch })
  } catch {}
}

export function setRepoSortMode(mode: SortMode): void {
  repoSortMode.set(mode)
  patchReposState({ repo_sort_mode: mode })
}

export function setCloneSortMode(mode: SortMode): void {
  cloneSortMode.set(mode)
  patchReposState({ clone_sort_mode: mode })
}

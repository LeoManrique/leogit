import { writable, derived } from 'svelte/store'

export type AppPhase = 'loading' | 'repo-picker' | 'main' | 'error'

export interface AppState {
  phase: AppPhase
  repos: string[]
  repoPath: string
  ghAuthed: boolean
  error: string
}

const initial: AppState = {
  phase: 'loading',
  repos: [],
  repoPath: '',
  ghAuthed: false,
  error: '',
}

export const appState = writable<AppState>(initial)

export const isLoading = derived(appState, ($state) => $state.phase === 'loading')
export const showRepoPicker = derived(appState, ($state) => $state.phase === 'repo-picker')
export const inMain = derived(appState, ($state) => $state.phase === 'main')
export const currentRepoPath = derived(appState, ($state) => $state.repoPath)

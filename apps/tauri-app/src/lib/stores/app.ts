import { writable } from 'svelte/store'

export type AppPhase = 'loading' | 'repo-picker' | 'main' | 'error'

export interface AppState {
  phase: AppPhase
  repos: string[]
  repoPath: string
  error: string
}

const initial: AppState = {
  phase: 'loading',
  repos: [],
  repoPath: '',
  error: '',
}

export const appState = writable<AppState>(initial)

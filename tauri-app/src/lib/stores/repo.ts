import { writable, derived } from 'svelte/store'
import type {
  FileEntry,
  CommitInfo,
  BranchInfo,
  DiffSelection,
  FileDiff,
} from '$lib/api/commands'

export type ActiveTab = 'changes' | 'history'

export interface RepoStatus {
  branch: string
  upstream: string
  hasUpstream: boolean
  ahead: number
  behind: number
  files: FileEntry[]
  isMerging: boolean
}

export interface RepoState {
  status: RepoStatus
  log: {
    commits: CommitInfo[]
    hasMore: boolean
    loaded: boolean
  }
  branches: BranchInfo[]
  selectedFiles: Set<string>
  userDeselected: Set<string>
  diffSelection: Map<string, DiffSelection>
  activeFile: FileEntry | null
  activeFileDiff: FileDiff | null
  isDiffLoading: boolean
  activeTab: ActiveTab
  activeCommit: CommitInfo | null
  activeCommitFiles: FileEntry[]
  activeCommitFile: FileEntry | null
  activeCommitFileDiff: FileDiff | null
  isCommitDiffLoading: boolean
  isLoading: boolean
  error?: string
}

const defaultStatus: RepoStatus = {
  branch: '',
  upstream: '',
  hasUpstream: false,
  ahead: 0,
  behind: 0,
  files: [],
  isMerging: false,
}

const defaultState: RepoState = {
  status: defaultStatus,
  log: {
    commits: [],
    hasMore: true,
    loaded: false,
  },
  branches: [],
  selectedFiles: new Set(),
  userDeselected: new Set(),
  diffSelection: new Map(),
  activeFile: null,
  activeFileDiff: null,
  isDiffLoading: false,
  activeTab: 'changes',
  activeCommit: null,
  activeCommitFiles: [],
  activeCommitFile: null,
  activeCommitFileDiff: null,
  isCommitDiffLoading: false,
  isLoading: false,
}

export const repoState = writable<RepoState>(defaultState)

export function resetRepoState() {
  repoState.set({
    ...defaultState,
    selectedFiles: new Set(),
    userDeselected: new Set(),
    diffSelection: new Map(),
  })
}

export const canCommit = derived(repoState, ($state) => {
  return $state.selectedFiles.size > 0 && !$state.isLoading
})

export const hasMergeConflicts = derived(repoState, ($state) => {
  return $state.status.files.some((f) => f.status === 'Conflicted')
})

export const currentBranch = derived(repoState, ($state) => {
  return $state.status.branch
})

export const remoteBranches = derived(repoState, ($state) => {
  return $state.branches.filter((b) => b.is_remote)
})

export const localBranches = derived(repoState, ($state) => {
  return $state.branches.filter((b) => !b.is_remote)
})

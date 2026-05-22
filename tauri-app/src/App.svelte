<script lang="ts">
  import { onMount } from 'svelte'
  import { appState } from '$lib/stores/app'
  import { refreshConfig } from '$lib/stores/config'
  import { ghApi, gitApi, configApi } from '$lib/api/commands'
  import { homeDir } from '@tauri-apps/api/path'
  import MainLayout from '$lib/views/MainLayout.svelte'
  import RepoPicker from '$lib/views/RepoPicker.svelte'

  onMount(() => {
    initializeApp()
  })

  async function initializeApp() {
    try {
      // Check gh auth in background — non-blocking. PR features will gate themselves.
      ghApi.checkAuth().then((authed) => {
        appState.update((s) => ({ ...s, ghAuthed: authed }))
      }).catch(() => {})

      // Load config + state (shared store so settings updates propagate)
      const cfg = await refreshConfig()
      const scanPaths = cfg?.scan_paths ?? []
      const scanDepth = cfg?.scan_depth ?? 3

      // Resolve scan paths: replace ~ with home, fallback to defaults if empty
      const home = await homeDir().catch(() => '')
      const resolved = scanPaths.length > 0
        ? scanPaths.map((p) => p.startsWith('~') ? p.replace(/^~/, home) : p)
        : home
          ? [`${home}/Dev`, `${home}/dev`, `${home}/code`, `${home}/Code`, `${home}/Projects`, `${home}/src`]
          : []

      const repos = await gitApi.discoverRepos(resolved, scanDepth)
      const state = await configApi.loadState().catch(() => ({ last_opened_repo: undefined }))

      if (state.last_opened_repo && repos.includes(state.last_opened_repo)) {
        appState.update((s) => ({ ...s, phase: 'main', repos, repoPath: state.last_opened_repo! }))
        return
      }
      if (repos.length === 1) {
        appState.update((s) => ({ ...s, phase: 'main', repos, repoPath: repos[0] }))
        configApi.saveState({ last_opened_repo: repos[0] }).catch(console.error)
        return
      }

      appState.update((s) => ({ ...s, phase: 'repo-picker', repos }))
    } catch (error) {
      appState.update((s) => ({ ...s, phase: 'error', error: String(error) }))
    }
  }

  function handleRepoSelect(repo: string) {
    appState.update((s) => ({ ...s, phase: 'main', repoPath: repo }))
    configApi.saveState({ last_opened_repo: repo }).catch(console.error)
  }
</script>

{#if $appState.phase === 'loading'}
  <div class="loading-screen">
    <div class="spinner"></div>
    <p>Discovering repositories…</p>
  </div>
{:else if $appState.phase === 'error'}
  <div class="error-screen">
    <h1>Something went wrong</h1>
    <pre>{$appState.error}</pre>
    <button onclick={() => { appState.update((s) => ({ ...s, phase: 'loading', error: '' })); initializeApp() }}>Retry</button>
  </div>
{:else if $appState.phase === 'repo-picker'}
  <RepoPicker repos={$appState.repos} onSelect={handleRepoSelect} />
{:else if $appState.phase === 'main' && $appState.repoPath}
  <MainLayout />
{/if}

<style>
  .loading-screen,
  .error-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100vh;
    gap: 14px;
    color: var(--text-secondary);
    background: var(--bg-primary);
    font-size: 13px;
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 2px solid var(--border-inactive);
    border-top-color: var(--border-active);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-screen h1 {
    font-size: 17px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .error-screen pre {
    background: var(--bg-secondary);
    padding: 10px 12px;
    border-radius: 6px;
    max-width: 720px;
    overflow: auto;
    white-space: pre-wrap;
    color: var(--status-red);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .error-screen button {
    padding: 3px 14px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
  }

  .error-screen button:hover {
    background: var(--surface-hover);
  }
</style>

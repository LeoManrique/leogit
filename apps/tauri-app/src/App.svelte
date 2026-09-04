<script lang="ts">
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { appState } from '$lib/stores/app'
  import { basename } from '$lib/utils/path'
  import { loadFileStatusStyles, refreshConfig, scanFolders } from '$lib/stores/config'
  import { gitApi, configApi, appApi, reposApi, type LaunchTarget } from '$lib/api/commands'
  import {
    hydrateReposState,
    patchReposState,
    recordRecentRepo,
  } from '$lib/stores/reposState'
  import { resolveCloneDefaultDir, rememberCloneDir } from '$lib/services/cloneFlow'
  import { updateChecker } from '$lib/services/updateChecker'
  import { dismissTopOverlay } from '$lib/actions/overlayStack'
  import MainLayout from '$lib/views/MainLayout.svelte'
  import RepoPicker from '$lib/views/RepoPicker.svelte'
  import CloneOverlay from '$lib/views/CloneOverlay.svelte'
  import SettingsOverlay from '$lib/views/SettingsOverlay.svelte'
  import HelpOverlay from '$lib/views/HelpOverlay.svelte'
  import Header from '$lib/components/Header.svelte'
  import InitRepoConfirm from '$lib/components/InitRepoConfirm.svelte'

  let unlistenOpenRepo: (() => void) | null = null

  // Settings for the pre-main phases. MainLayout owns its own instance (it's
  // part of that view's Escape stack); this one exists because the picker and
  // error screens are otherwise a dead end — a user whose scan paths match
  // nothing can't reach the setting that would fix it.
  let showSettings = $state(false)
  let showHelp = $state(false)

  // Clone from the picker phase. Same reasoning as Settings above, and sharper:
  // clone used to live only inside the main-view dropdown, so the first-run user
  // with nothing to open — the one most likely to want to clone — was the one
  // user who couldn't reach it.
  let showClone = $state(false)
  let cloneDefaultDir = $state('~/Dev')

  // `leogit <dir>` pointed at a folder that isn't a repository yet. The prompt
  // lives here rather than in MainLayout because it isn't scoped to the open
  // repo — it can appear over the picker, over another repo, or at first launch.
  let initPath = $state<string | null>(null)
  let isInitializing = $state(false)
  let initError = $state('')

  // Set while MainLayout is mounted so a repo opened from the prompt can reuse
  // its live switch (which resets the view state) instead of racing it.
  let mainLayout = $state<{ openExternalRepo: (path: string) => Promise<void> } | null>(null)

  onMount(() => {
    initializeApp()
    // Once-per-session release check — lives here, not in MainLayout, so it
    // also runs while the app sits in the repo picker.
    updateChecker.start()
    // Warm-start `leogit <dir>`: a second invocation focuses this window and
    // emits `open-repo`. While in 'main', MainLayout owns the live repo switch
    // (it must reset its own view state); here we handle the pre-main phases —
    // and, in every phase, a target that isn't a repository yet.
    listen<LaunchTarget>('open-repo', (e) => handleLaunchTarget(e.payload)).then((u) => {
      unlistenOpenRepo = u
    })
    return () => {
      updateChecker.stop()
      unlistenOpenRepo?.()
    }
  })

  function handleLaunchTarget(target: LaunchTarget) {
    if (!target?.path) return
    if (!target.is_repo) {
      promptInit(target.path)
      return
    }
    if (get(appState).phase === 'main') return
    enterRepo(target.path)
  }

  /** Switch the app into `main` on a repo, adding it if it wasn't discovered. */
  function enterRepo(path: string) {
    appState.update((s) => ({
      ...s,
      phase: 'main',
      repos: s.repos.includes(path) ? s.repos : [...s.repos, path],
      repoPath: path,
    }))
    patchReposState({ last_opened_repo: path })
    recordRecentRepo(path)
  }

  function promptInit(path: string) {
    console.log('[launch] not a repository, offering to create one:', path)
    initError = ''
    isInitializing = false
    initPath = path
  }

  async function confirmInit() {
    if (!initPath || isInitializing) return
    isInitializing = true
    initError = ''
    try {
      // Returns the path to open — the folder itself, or the enclosing repo if
      // one appeared in the meantime.
      const repoPath = await gitApi.initRepo(initPath)
      initPath = null
      // In 'main' the mounted MainLayout owns switching; otherwise nothing is
      // showing a repo yet, so move the app into 'main' ourselves.
      if (get(appState).phase === 'main' && mainLayout) {
        await mainLayout.openExternalRepo(repoPath)
      } else {
        enterRepo(repoPath)
      }
    } catch (error) {
      initError = String(error)
    } finally {
      isInitializing = false
    }
  }

  async function initializeApp() {
    try {
      // No gh auth probe here. `ghApi.checkAuth` stays wired for a surface that
      // needs to gate on the answer, but the answer belongs at the point of use
      // rather than at launch: signing in happens outside this app, so one read
      // at startup is stale for the whole session. The gh surfaces there are
      // today — the clone list, publish — deliberately don't probe at all and
      // let gh's own "Run `gh auth login`" carry the failure, which names the
      // fix better than a disabled control would.

      // Load config + state (shared store so settings updates propagate).
      // Scan-path resolution (~ expansion, stock folders when the list is
      // empty) lives in core, next to the walker that uses it — as does the
      // union with the persisted MRU, which is what keeps a clone, a CLI open
      // or an Open-Other repo listed across restarts.
      const cfg = await refreshConfig()
      // Before the picker can render: it carries the sort toggle, and a list
      // that comes up in the default order and then reshuffles once the main
      // view hydrates is a list that ignored the user's choice.
      await hydrateReposState()
      // The status glyph table, once. Awaited here so no changed-file row ever
      // paints before it lands.
      await loadFileStatusStyles()
      const repos = await reposApi.knownRepos(cfg?.scan_paths ?? [], cfg?.scan_depth ?? 3)
      const state = await configApi.loadState().catch(() => ({ last_opened_repo: undefined }))

      // A repo passed on the cold-start command line (`leogit <dir>`) wins over
      // the remembered/last-opened repo. It may live outside the scan paths, so
      // add it to the list even if discovery didn't surface it.
      const launchTarget = await appApi.takePendingLaunchTarget().catch(() => null)
      if (launchTarget?.is_repo) {
        console.log('[launch] opening repo from command line:', launchTarget.path)
        appState.update((s) => ({ ...s, repos }))
        enterRepo(launchTarget.path)
        return
      }
      // Not a repository yet: prompt to create one, and keep resolving so the
      // prompt lands over the picker (or the last repo) rather than a blank app.
      if (launchTarget) promptInit(launchTarget.path)

      // Reopen where the user left off. `known_repos` already unions discovery
      // with the existence-checked MRU, so anything it lists is openable — but
      // the remembered repo can legitimately be absent from that list (aged out
      // of the MRU's cap while living outside the scan paths, or last opened by
      // the other client). Conditioning the restore on discovery re-finding it
      // dropped those users into the picker with the repo they were just in
      // missing from it, so verify the path instead and list it ourselves.
      const last = state.last_opened_repo
      if (last) {
        const listed = repos.includes(last)
        if (listed || (await gitApi.isGitRepo(last).catch(() => false))) {
          appState.update((s) => ({
            ...s,
            phase: 'main',
            repos: listed ? repos : [...repos, last],
            repoPath: last,
          }))
          return
        }
      }
      if (repos.length === 1) {
        appState.update((s) => ({ ...s, phase: 'main', repos, repoPath: repos[0] }))
        // Read-modify-write so we don't clobber sort modes / recent_repos.
        patchReposState({ last_opened_repo: repos[0] })
        return
      }

      appState.update((s) => ({ ...s, phase: 'repo-picker', repos }))
    } catch (error) {
      appState.update((s) => ({ ...s, phase: 'error', error: String(error) }))
    }
  }

  function handleRepoSelect(repo: string) {
    appState.update((s) => ({ ...s, phase: 'main', repoPath: repo }))
    // Read-modify-write so we don't clobber sort modes / recent_repos.
    patchReposState({ last_opened_repo: repo })
  }

  async function openClone() {
    cloneDefaultDir = await resolveCloneDefaultDir()
    showClone = true
  }

  /** A clone finished: remember where it landed, then open it. */
  async function handleCloned(repoPath: string, parentDir: string) {
    showClone = false
    await rememberCloneDir(parentDir)
    enterRepo(repoPath)
  }

  /**
   * The window is named after the repository it is showing, which is what the
   * native client's title bar has always said and what the window list, the
   * Dock and ⌘` between two leogit windows have to read to tell them apart.
   * The name is the folder's, matching core's `get_repo_name`; back on the
   * picker there is nothing to name, so it falls back to the product.
   */
  $effect(() => {
    const path = $appState.phase === 'main' ? $appState.repoPath : ''
    void getCurrentWindow()
      .setTitle(path ? basename(path) : 'leogit')
      .catch((e) => console.warn('[window] title update failed', e))
  })

  // Mirrors MainLayout's shortcuts so the two phases behave the same. Only
  // bound outside `main`, where MainLayout owns these keys — binding in both
  // would double-handle them.
  function handlePreMainKeyDown(e: KeyboardEvent) {
    if (get(appState).phase === 'main') return
    const target = e.target as HTMLElement | null
    const inField = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement

    // The overlay stack, not a second list of flags: each surface registers its
    // own dismissal, so the pre-main phases and the main view answer Escape
    // through the same mechanism instead of two hand-kept copies that drifted.
    if (e.key === 'Escape') {
      if (dismissTopOverlay()) e.preventDefault()
    } else if (e.key === ',' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault()
      showSettings = true
    } else if (e.key === '?' && !inField) {
      // The picker's search box is autofocused, so an unguarded '?' would
      // swallow the character the user meant to type into it.
      e.preventDefault()
      showHelp = true
    }
  }
</script>

<svelte:window onkeydown={handlePreMainKeyDown} />

{#if $appState.phase === 'main' && $appState.repoPath}
  <MainLayout bind:this={mainLayout} />
{:else}
  <!-- Same header bar as the main view, minus everything that acts on a repo.
       Without it the picker is a dead end: a user whose scan paths match
       nothing has no route to the setting that would fix it. -->
  <div class="pre-main">
    <Header
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
    />
    <div class="pre-main-body">
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
        <RepoPicker
          repos={$appState.repos}
          onSelect={handleRepoSelect}
          onOpenSettings={() => (showSettings = true)}
          onClone={openClone}
          scannedPaths={$scanFolders}
        />
      {/if}
    </div>
  </div>

  <CloneOverlay
    isOpen={showClone}
    defaultDir={cloneDefaultDir}
    onClose={() => (showClone = false)}
    onCloned={handleCloned}
  />
  <SettingsOverlay isOpen={showSettings} onClose={() => (showSettings = false)} />
  <HelpOverlay isOpen={showHelp} onClose={() => (showHelp = false)} />
{/if}

{#if initPath}
  <InitRepoConfirm
    path={initPath}
    {isInitializing}
    error={initError}
    onConfirm={confirmInit}
    onCancel={() => (initPath = null)}
  />
{/if}

<style>
  /* Header on top, phase content filling the rest. The body is the positioning
     context for the picker, which fills it rather than the viewport so the
     header's controls stay clickable. */
  .pre-main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-primary);
  }

  .pre-main-body {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .loading-screen,
  .error-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
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
    font-size: 15px;
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

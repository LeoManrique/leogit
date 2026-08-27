<script lang="ts">
  import { autofocus } from '$lib/actions/autofocus'
  import { nextActiveIndex, scrollIntoViewWhenActive } from '$lib/actions/listNavigation'
  import { reposApi } from '$lib/api/commands'
  import { basename } from '$lib/utils/path'
  import { discoveringRepos } from '$lib/services/repoDiscovery'
  import RepoListEmptyState from '$lib/components/RepoListEmptyState.svelte'

  interface Props {
    repos: string[]
    onSelect: (repo: string) => void
    /** Opens Settings, for the empty state's call to action. The app header
     *  above carries the persistent entry point; this is the contextual one. */
    onOpenSettings: () => void
    /** Opens the Clone dialog. The user least likely to have a repo to open is
     *  the one most likely to want to clone one, so this phase needs the entry
     *  as much as the main-view dropdown does. */
    onClone: () => void
    /** Folders discovery actually searched: named by the empty state, and
     *  trimmed off a repo's path before the filter searches it. */
    scannedPaths?: string[]
  }

  let { repos = [], onSelect, onOpenSettings, onClone, scannedPaths = [] }: Props = $props()

  let searchInput = $state('')

  /*
    Rows to show, best match first. Core ranks them — one crossing per
    keystroke, not one per row — and keeps the input order within a tier, so
    discovery's order survives filtering. Enter picks the highlighted row,
    which starts on the top match, so the ranking is what it acts on.

    The rule lives in core because this client and the native one had already
    drifted on it, including on the very set of labels they searched.
  */
  let filteredRepos = $state<string[]>([])

  $effect(() => {
    const query = searchInput
    const rows = repos
    const folders = scannedPaths
    if (!query.trim()) {
      filteredRepos = rows
      return
    }
    let cancelled = false
    reposApi
      .filterRepos(
        query,
        rows.map((path) => ({ path, names: [basename(path)] })),
        folders
      )
      .then((matched) => {
        if (!cancelled) filteredRepos = matched
      })
      .catch(() => {
        if (!cancelled) filteredRepos = rows
      })
    return () => {
      cancelled = true
    }
  })

  function handleSelect(repo: string) {
    onSelect(repo)
  }

  // Keyboard cursor over the filtered list; snaps back to the top match each
  // time the query changes so Enter targets a sensible default.
  let activeIndex = $state(0)
  $effect(() => {
    searchInput
    activeIndex = 0
  })

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      searchInput = ''
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filteredRepos.length, 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filteredRepos.length, -1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const repo = filteredRepos[activeIndex]
      if (repo) handleSelect(repo)
    }
  }
</script>

<div class="repo-picker-overlay">
  <div class="repo-picker-modal">
    <h2 class="modal-header">Select Repository</h2>

    <div class="search-section">
      <input
        type="text"
        class="search-input"
        placeholder="Search repositories... (fuzzy match)"
        bind:value={searchInput}
        onkeydown={handleKeyDown}
        use:autofocus
      />
    </div>

    <div class="repos-list">
      {#if filteredRepos.length === 0}
        <RepoListEmptyState
          discovering={$discoveringRepos && repos.length === 0}
          hasRepos={repos.length > 0}
          {scannedPaths}
          {onOpenSettings}
        />
      {:else}
        {#each filteredRepos as repo, i (repo)}
          <button
            class="repo-item"
            class:active={i === activeIndex}
            use:scrollIntoViewWhenActive={i === activeIndex}
            onclick={() => handleSelect(repo)}
          >
            <span class="repo-path">{repo}</span>
          </button>
        {/each}
      {/if}
    </div>

    <!--
      Same footer as the main-view dropdown. This phase is where a first-run
      user lands, so it is the one that most needs it — cloning used to be
      reachable only from inside a repo, which is the one place you no longer
      need it.
    -->
    <div class="footer">
      <button class="footer-btn" onclick={onClone}>Clone Repository…</button>
    </div>
  </div>
</div>

<style>
  /* Fills its container rather than the viewport: the app header sits above
     it in the pre-main phases, and a fixed overlay would cover the Settings
     and Help buttons that are the only way out of an empty picker. */
  .repo-picker-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1;
  }

  .repo-picker-modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 520px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .modal-header {
    padding: 14px 16px 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-inactive);
    margin: 0;
  }

  .search-section {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .search-input {
    width: 100%;
    padding: 4px 8px;
    font-size: 13px;
    border-radius: 6px;
  }

  .repos-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  /* Same footer as the main-view dropdown, so the two pickers read as one
     component family: a trailing text button. */
  .footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 8px 10px;
    border-top: 1px solid var(--border-inactive);
  }

  .footer-btn {
    padding: 3px 8px;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-family: inherit;
    font-size: 12px;
    white-space: nowrap;
    transition:
      color 100ms ease,
      background 100ms ease;
  }

  .footer-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .repo-item {
    display: flex;
    align-items: center;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    transition: background 100ms ease;
    text-align: left;
    font-size: 13px;
  }

  .repo-item:hover {
    background: var(--surface-hover);
  }

  .repo-item:active {
    background: var(--bg-tertiary);
  }

  /* Keyboard cursor (arrow-key highlight); a ring so it reads as "focused"
     and stays distinct from the hover fill. */
  .repo-item.active {
    box-shadow: inset 0 0 0 1.5px var(--border-active);
  }

  .repo-path {
    color: var(--text-primary);
    font-size: 13px;
    word-break: break-all;
  }
</style>

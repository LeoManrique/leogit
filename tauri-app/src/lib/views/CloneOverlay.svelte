<script lang="ts">
  import { ghApi, gitApi, type GhRepo } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'

  interface Props {
    isOpen: boolean
    /** Pre-filled destination folder (last-used clone dir, else first scan path). */
    defaultDir: string
    onClose: () => void
    /** Fired after a successful clone with the new repo path and the parent
        folder it landed in (so the caller can remember it and open the repo). */
    onCloned: (repoPath: string, parentDir: string) => void
  }

  let { isOpen, defaultDir, onClose, onCloned }: Props = $props()

  type Tab = 'github' | 'url'
  let tab = $state<Tab>('github')

  // Shared destination + in-flight/clone-error state.
  let destDir = $state('')
  let isCloning = $state(false)
  let cloneError = $state('')

  // GitHub tab state.
  let repos = $state<GhRepo[]>([])
  let reposLoading = $state(false)
  let reposError = $state('')
  let reposLoaded = $state(false)
  let ghFilter = $state('')
  let selectedRepo = $state<GhRepo | null>(null)
  let sortBy = $state<'recent' | 'name'>('recent')

  // URL tab state.
  let url = $state('')

  // Re-arm the dialog every time it opens: reset transient state and seed the
  // destination from the caller's remembered folder.
  $effect(() => {
    if (isOpen) {
      destDir = defaultDir
      cloneError = ''
      // Lazily pull the user's repos the first time the GitHub tab is shown.
      if (tab === 'github' && !reposLoaded && !reposLoading) loadRepos()
    }
  })

  async function loadRepos() {
    reposLoading = true
    reposError = ''
    try {
      repos = await ghApi.repoList(200)
      reposLoaded = true
    } catch (e) {
      reposError = String(e)
    } finally {
      reposLoading = false
    }
  }

  function selectTab(next: Tab) {
    tab = next
    cloneError = ''
    if (next === 'github' && !reposLoaded && !reposLoading) loadRepos()
  }

  const filteredRepos = $derived.by(() => {
    const q = ghFilter.trim().toLowerCase()
    const list = q
      ? repos.filter((r) => r.name_with_owner.toLowerCase().includes(q))
      : [...repos]
    // Recently-modified (newest pushedAt first — ISO-8601 sorts lexically) or
    // alphabetical by repo name (case-insensitive so casing doesn't split groups).
    return sortBy === 'name'
      ? list.sort((a, b) =>
          a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
        )
      : list.sort((a, b) => b.pushed_at.localeCompare(a.pushed_at))
  })

  /** Strip a trailing `.git`/slash and return the final path segment as a folder name. */
  function repoNameFromUrl(raw: string): string {
    const trimmed = raw.trim().replace(/\/+$/, '').replace(/\.git$/i, '')
    if (!trimmed) return ''
    // owner/repo shorthand → repo
    if (/^[\w.-]+\/[\w.-]+$/.test(trimmed)) return trimmed.split('/')[1]
    const m = trimmed.match(/[/:]([\w.-]+?)$/)
    return m ? m[1] : ''
  }

  /** Expand `owner/repo` shorthand to a full github.com URL; pass anything else through. */
  function normalizeUrl(raw: string): string {
    const trimmed = raw.trim()
    if (/^[\w.-]+\/[\w.-]+$/.test(trimmed.replace(/\.git$/i, ''))) {
      return `https://github.com/${trimmed.replace(/\.git$/i, '')}`
    }
    return trimmed
  }

  // The folder name the clone will create, and the full path it lands at.
  const repoName = $derived(
    tab === 'github' ? (selectedRepo?.name ?? '') : repoNameFromUrl(url),
  )
  const targetPath = $derived(
    repoName ? `${destDir.replace(/\/+$/, '')}/${repoName}` : '',
  )

  const canClone = $derived(
    !isCloning && destDir.trim().length > 0 && repoName.length > 0,
  )

  function handleOverlayKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isCloning) onClose()
  }

  async function handleClone() {
    if (!canClone) return
    isCloning = true
    cloneError = ''
    const parentDir = destDir.replace(/\/+$/, '')
    try {
      const repoPath =
        tab === 'github' && selectedRepo
          ? await ghApi.clone(selectedRepo.name_with_owner, targetPath)
          : await gitApi.cloneRepo(normalizeUrl(url), targetPath)
      onCloned(repoPath, parentDir)
    } catch (e) {
      cloneError = String(e)
    } finally {
      isCloning = false
    }
  }
</script>

{#if isOpen}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget && !isCloning) onClose()
    }}
    onkeydown={handleOverlayKeyDown}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <div class="modal-header">
        <h2>Clone a repository</h2>
        <button class="close-btn" onclick={onClose} disabled={isCloning} aria-label="Close">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
            <line x1="4" y1="4" x2="12" y2="12" />
            <line x1="12" y1="4" x2="4" y2="12" />
          </svg>
        </button>
      </div>

      <div class="tabs" role="tablist">
        <button
          class="tab"
          class:active={tab === 'github'}
          role="tab"
          aria-selected={tab === 'github'}
          onclick={() => selectTab('github')}
        >
          GitHub
        </button>
        <button
          class="tab"
          class:active={tab === 'url'}
          role="tab"
          aria-selected={tab === 'url'}
          onclick={() => selectTab('url')}
        >
          URL
        </button>
      </div>

      <div class="modal-body">
        {#if tab === 'github'}
          <div class="gh-toolbar">
            <input
              type="text"
              class="text-input"
              placeholder="Filter your repositories…"
              bind:value={ghFilter}
              use:autofocus
            />
            <button
              class="sort-btn"
              onclick={() => (sortBy = sortBy === 'recent' ? 'name' : 'recent')}
              title={sortBy === 'recent' ? 'Sorted by recently modified' : 'Sorted alphabetically'}
              aria-label={sortBy === 'recent' ? 'Sorted by recently modified' : 'Sorted alphabetically'}
            >
              {#if sortBy === 'recent'}
                <!-- Clock + down arrow = most-recently-modified first (mirrors the
                     A→Z glyph's arrow so the two modes read as a matched pair). -->
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <circle cx="4.25" cy="8" r="4" />
                  <path d="M4.25 5.5V8l1.5 0.9" />
                  <path d="M12.5 3.5v8" />
                  <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
                </svg>
              {:else}
                <!-- A→Z = alphabetical by name. -->
                <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
                  <text x="0.5" y="6.6" font-size="6.5" font-weight="700" fill="currentColor" font-family="-apple-system, system-ui, sans-serif">A</text>
                  <text x="0.5" y="14.8" font-size="6.5" font-weight="700" fill="currentColor" font-family="-apple-system, system-ui, sans-serif">Z</text>
                  <g fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M12.5 3.5v8" />
                    <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
                  </g>
                </svg>
              {/if}
            </button>
          </div>
          <div class="repo-list">
            {#if reposLoading}
              <div class="list-note">Loading your repositories…</div>
            {:else if reposError}
              <div class="list-note error">
                {reposError}
                <button class="retry" onclick={loadRepos}>Retry</button>
              </div>
            {:else if filteredRepos.length === 0}
              <div class="list-note">No repositories found.</div>
            {:else}
              {#each filteredRepos as repo (repo.name_with_owner)}
                <button
                  class="repo-row"
                  class:selected={selectedRepo?.name_with_owner === repo.name_with_owner}
                  onclick={() => (selectedRepo = repo)}
                >
                  <span class="repo-nwo">{repo.name_with_owner}</span>
                  {#if repo.is_private}<span class="private-badge">Private</span>{/if}
                </button>
              {/each}
            {/if}
          </div>
        {:else}
          <label class="field-label" for="clone-url">Repository URL or <code>owner/name</code></label>
          <input
            id="clone-url"
            type="text"
            class="text-input"
            placeholder="https://github.com/owner/repo.git"
            bind:value={url}
            use:autofocus
          />
        {/if}

        <label class="field-label" for="clone-dest">Local path</label>
        <input id="clone-dest" type="text" class="text-input mono" bind:value={destDir} />
        {#if targetPath}
          <div class="path-preview">Clones into <code>{targetPath}</code></div>
        {/if}

        {#if cloneError}
          <div class="clone-error">{cloneError}</div>
        {/if}
      </div>

      <div class="modal-footer">
        <button class="btn-secondary" onclick={onClose} disabled={isCloning}>Cancel</button>
        <button class="btn-primary" onclick={handleClone} disabled={!canClone}>
          {isCloning ? 'Cloning…' : 'Clone'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 480px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .close-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 4px;
  }

  .close-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  /* Tab strip — mirrors the Changes/History TabBar exactly (same padding,
     13px label, underline-on-active flush with the strip's bottom border) so
     the dialog's tabs read as the same component family. */
  .tabs {
    display: flex;
    gap: 0;
    padding: 0 8px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .tab {
    flex: 0 0 auto;
    padding: 8px 14px;
    font-size: 13px;
    font-weight: 400;
    color: var(--text-muted);
    background: transparent;
    border: none;
    border-radius: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    cursor: pointer;
    transition: color 120ms;
  }

  .tab:hover {
    color: var(--text-secondary);
  }

  .tab.active {
    color: var(--text-primary);
    font-weight: 600;
    border-bottom-color: var(--border-active);
  }

  .modal-body {
    flex: 1;
    padding: 14px 16px 16px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field-label {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 4px;
  }

  .field-label code {
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .text-input {
    width: 100%;
    padding: 5px 8px;
    font-size: 13px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .text-input.mono {
    font-family: var(--font-mono);
    font-size: 12px;
  }

  /* Filter + sort sit on one row above the repo list. */
  .gh-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .gh-toolbar .text-input {
    flex: 1;
    min-width: 0;
  }

  /* Icon-only sort toggle; the glyph (clock / A→Z) is itself the state label. */
  .sort-btn {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    transition:
      color 100ms ease,
      background 100ms ease;
  }

  .sort-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .text-input:focus {
    outline: none;
    border-color: var(--border-active);
  }

  .repo-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 260px;
    overflow-y: auto;
    border: 1px solid var(--border-inactive);
    border-radius: 6px;
    padding: 4px;
  }

  .list-note {
    padding: 16px 12px;
    text-align: center;
    color: var(--text-faint);
    font-size: 12px;
  }

  .list-note.error {
    color: var(--status-red);
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
  }

  .retry {
    padding: 2px 10px;
    font-size: 11px;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
  }

  .repo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 26px;
    background: transparent;
    border: none;
    border-radius: 5px;
    cursor: pointer;
    text-align: left;
  }

  .repo-row:hover {
    background: var(--surface-hover);
  }

  .repo-row.selected {
    background: var(--bg-tertiary);
  }

  .repo-nwo {
    flex: 1;
    font-size: 13px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .private-badge {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--badge-fg);
    background: var(--badge-bg);
    padding: 1px 6px;
    border-radius: 5px;
  }

  .path-preview {
    font-size: 11px;
    color: var(--text-muted);
  }

  .path-preview code {
    font-family: var(--font-mono);
    color: var(--text-secondary);
  }

  .clone-error {
    font-size: 12px;
    color: var(--status-red);
    word-break: break-word;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary,
  .btn-primary {
    padding: 3px 14px;
    font-size: 12px;
    font-weight: 500;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    cursor: pointer;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn-primary {
    background: var(--border-active);
    color: #ffffff;
    border-color: var(--border-active);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-primary:disabled,
  .btn-secondary:disabled,
  .close-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

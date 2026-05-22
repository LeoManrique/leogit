<script lang="ts">
  interface Props {
    repos: string[]
    onSelect: (repo: string) => void
  }

  let { repos = [], onSelect }: Props = $props()

  let searchInput = $state('')

  function filterRepos(query: string): string[] {
    if (!query.trim()) return repos

    const lower = query.toLowerCase()
    return repos.filter((repo) => {
      const lowerRepo = repo.toLowerCase()
      // Fuzzy match: check if all characters in query appear in repo in order
      let queryIdx = 0
      for (let i = 0; i < lowerRepo.length && queryIdx < lower.length; i++) {
        if (lowerRepo[i] === lower[queryIdx]) {
          queryIdx++
        }
      }
      return queryIdx === lower.length
    })
  }

  function handleSelect(repo: string) {
    onSelect(repo)
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      searchInput = ''
    }
  }

  const filteredRepos = $derived(filterRepos(searchInput))
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
        autofocus
      />
    </div>

    <div class="repos-list">
      {#if filteredRepos.length === 0}
        <div class="empty-repos">
          {#if repos.length === 0}
            <p>No repositories found</p>
          {:else}
            <p>No matching repositories</p>
          {/if}
        </div>
      {:else}
        {#each filteredRepos as repo (repo)}
          <button class="repo-item" onclick={() => handleSelect(repo)}>
            <span class="repo-path">{repo}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .repo-picker-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
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
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  .empty-repos {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
    padding: 40px 20px;
    text-align: center;
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

  .repo-path {
    color: var(--text-primary);
    font-size: 13px;
    word-break: break-all;
  }
</style>

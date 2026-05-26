<script lang="ts">
  interface Props {
    repos: string[]
    currentRepo: string
    onSelect: (repo: string) => void
  }

  let { repos = [], currentRepo = '', onSelect }: Props = $props()

  let filter = $state('')

  function basename(path: string): string {
    const parts = path.split('/').filter(Boolean)
    return parts[parts.length - 1] || path
  }

  function parentName(path: string): string {
    const parts = path.split('/').filter(Boolean)
    return parts.length >= 2 ? parts[parts.length - 2] : ''
  }

  function fuzzyMatch(query: string, target: string): boolean {
    const q = query.toLowerCase()
    const t = target.toLowerCase()
    let i = 0
    for (let j = 0; j < t.length && i < q.length; j++) {
      if (t[j] === q[i]) i++
    }
    return i === q.length
  }

  const sortedRepos = $derived.by(() => {
    const list = [...repos].sort((a, b) =>
      basename(a).localeCompare(basename(b), undefined, { sensitivity: 'base' }),
    )
    if (currentRepo) {
      const idx = list.indexOf(currentRepo)
      if (idx > 0) {
        list.splice(idx, 1)
        list.unshift(currentRepo)
      }
    }
    return list
  })

  const filteredRepos = $derived.by(() => {
    const q = filter.trim()
    if (!q) return sortedRepos
    return sortedRepos.filter((p) => fuzzyMatch(q, basename(p)) || fuzzyMatch(q, p))
  })

  function handleSelect(repo: string) {
    if (repo === currentRepo) return
    onSelect(repo)
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && filteredRepos.length > 0) {
      e.preventDefault()
      handleSelect(filteredRepos[0])
    }
  }
</script>

<div class="repo-dropdown">
  <div class="filter-row">
    <input
      type="text"
      class="filter-input"
      placeholder="Filter repositories…"
      bind:value={filter}
      onkeydown={handleKeyDown}
      autofocus
    />
  </div>

  <div class="repo-list">
    {#if filteredRepos.length === 0}
      <div class="empty">No repositories</div>
    {:else}
      {#each filteredRepos as repo (repo)}
        {@const isCurrent = repo === currentRepo}
        <button
          class="repo-item"
          class:current={isCurrent}
          onclick={() => handleSelect(repo)}
          title={repo}
        >
          <span class="repo-name">{basename(repo)}</span>
          <span class="repo-parent">{parentName(repo)}</span>
        </button>
      {/each}
    {/if}
  </div>
</div>

<style>
  .repo-dropdown {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 340px;
    max-height: 420px;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .filter-row {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .filter-input {
    width: 100%;
    padding: 4px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 13px;
  }

  .filter-input:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  .repo-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }

  .empty {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-faint);
    font-size: 12px;
  }

  .repo-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    transition: background 100ms ease;
  }

  .repo-item:hover {
    background: var(--surface-hover);
  }

  .repo-item.current {
    background: var(--bg-tertiary);
  }

  .repo-name {
    flex: 1;
    color: var(--text-primary);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-item.current .repo-name {
    font-weight: 500;
  }

  .repo-parent {
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
    flex-shrink: 0;
  }
</style>

<script lang="ts">
  import { repoState } from '$lib/stores/repo'

  function switchTab(tab: 'changes' | 'history') {
    repoState.update((state) => ({
      ...state,
      activeTab: tab,
    }))
  }
</script>

<div class="tab-bar">
  <button
    class="tab"
    class:active={$repoState.activeTab === 'changes'}
    onclick={() => switchTab('changes')}
  >
    Changes
    {#if $repoState.status.files.length > 0}
      <span class="count-badge">{$repoState.status.files.length}</span>
    {/if}
  </button>
  <button
    class="tab"
    class:active={$repoState.activeTab === 'history'}
    onclick={() => switchTab('history')}
  >
    History
  </button>
</div>

<style>
  .tab-bar {
    display: flex;
    /* Match the Header's height so the Changes/History tabs sit on the same
       horizontal baseline as the repo chips and Pull/Push buttons next door. */
    height: 40px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-secondary);
    gap: 0;
    padding: 0 8px;
  }

  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
    padding: 8px 14px;
    font-size: 13px;
    font-weight: 400;
    cursor: pointer;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
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

  /*
    Count of changed files. Sits inline next to the tab label so the user can
    see the working-tree size even from the History tab. Mirrors the inspo
    app's pill — neutral grey, semi-bold, tabular figures so the width is
    stable as the count grows/shrinks.
  */
  .count-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 16px;
    padding: 0 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
    border-radius: 8px;
    font-size: 11px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
</style>

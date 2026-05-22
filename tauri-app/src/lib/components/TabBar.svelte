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
    height: 36px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-primary);
    gap: 0;
  }

  .tab {
    flex: 0 0 auto;
    padding: 8px 16px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-bottom: 2px solid transparent;
    transition: color 150ms, border-color 150ms;
  }

  .tab:hover {
    color: var(--text-primary);
  }

  .tab.active {
    color: var(--text-primary);
    border-bottom-color: var(--border-active);
  }
</style>

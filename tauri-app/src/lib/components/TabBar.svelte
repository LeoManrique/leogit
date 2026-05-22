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
    height: 34px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-secondary);
    gap: 0;
    padding: 0 8px;
  }

  .tab {
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
</style>

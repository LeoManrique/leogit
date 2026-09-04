<script lang="ts">
  import { repoState, setActiveTab } from '$lib/stores/repo'
</script>

<div class="tab-bar">
  <button
    class="tab"
    class:active={$repoState.activeTab === 'changes'}
    onclick={() => setActiveTab('changes')}
  >
    Changes
    {#if $repoState.status.files.length > 0}
      <span class="count-badge">{$repoState.status.files.length}</span>
    {/if}
  </button>
  <button
    class="tab"
    class:active={$repoState.activeTab === 'history'}
    onclick={() => setActiveTab('history')}
  >
    History
  </button>
</div>

<style>
  /* 36px, and it is `RepoTabBar`'s own arithmetic rather than a number chosen
     to line up with something: a 13px label's 16px line box under
     `.padding(.vertical, 10)` (RepoTabBar.swift:41), inside a strip whose only
     other inset is `.padding(.horizontal, 8)` (`:24`).

     It used to be 40px to stand level with the header beside it, which is a
     rationale that no longer exists — the header spans the window now and sits
     *above* this strip rather than next to it, so the tab bar answers to the
     native tab bar and to nothing else. */
  .tab-bar {
    display: flex;
    height: 36px;
    flex-shrink: 0;
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
    padding: 10px 14px;
    font-size: 13px;
    font-weight: 400;
    cursor: pointer;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: 0;
    /* The active underline is an inset shadow rather than a border, because
       natively it is an `.overlay(alignment: .bottom)` (RepoTabBar.swift:45-52)
       — it is painted *over* the tab's bottom edge and takes none of its
       height. A `border-bottom` would eat 2px out of a border-box tab and push
       the label a pixel off centre, and only on the active one, so the strip
       would shift as the tab changed. It hangs 1px below the strip's own
       hairline so the two fuse, which is what the native comment describes. */
    box-shadow: inset 0 -2px 0 transparent;
    margin-bottom: -1px;
    transition: color 120ms;
  }

  .tab:hover {
    color: var(--text-secondary);
  }

  .tab.active {
    color: var(--text-primary);
    font-weight: 600;
    box-shadow: inset 0 -2px 0 var(--border-active);
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

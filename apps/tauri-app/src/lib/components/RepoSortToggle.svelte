<script lang="ts">
  import { repoSortMode, setRepoSortMode } from '$lib/stores/reposState'
  import Icon from './Icon.svelte'

  /*
    The repo lists' clock ⇄ A→Z toggle, shared by the startup picker and the
    header switcher — the two are one surface, and a toggle on only one of them
    writes a `repo_sort_mode` the other silently ignores.

    The glyph is the state label, so the sentence lives in the tooltip. Recency
    here is when you last *opened* a repository, which is the question a
    switcher is actually asked — the Clone dialog's identical control means last
    *pushed*, and says so in its own tooltip.
  */

  const label = $derived(
    $repoSortMode === 'recent' ? 'Sorted by recently opened' : 'Sorted alphabetically',
  )

  function toggle() {
    setRepoSortMode($repoSortMode === 'recent' ? 'name' : 'recent')
  }
</script>

<button class="icon-btn" onclick={toggle} title={label} aria-label={label}>
  <!--
    A bare clock and a bare "abc", as `RepoPickerList.swift:245` draws them.
    Both used to carry a descending arrow as well, which contradicted the note
    above: if the glyph is the state label and the sentence lives in the
    tooltip, a second mark encoding a direction the control cannot even change
    is one claim too many.
  -->
  {#if $repoSortMode === 'recent'}
    <Icon name="clock" size={16} />
  {:else}
    <Icon name="textformat-abc" size={16} />
  {/if}
</button>

<style>
  .icon-btn {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
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

  .icon-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }
</style>

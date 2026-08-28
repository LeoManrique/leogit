<script lang="ts">
  import { repoSortMode, setRepoSortMode } from '$lib/stores/reposState'

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
  {#if $repoSortMode === 'recent'}
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <circle cx="4.25" cy="8" r="4" />
      <path d="M4.25 5.5V8l1.5 0.9" />
      <path d="M12.5 3.5v8" />
      <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
    </svg>
  {:else}
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <text
        x="0.5"
        y="6.6"
        font-size="6.5"
        font-weight="700"
        fill="currentColor"
        font-family="-apple-system, system-ui, sans-serif">A</text
      >
      <text
        x="0.5"
        y="14.8"
        font-size="6.5"
        font-weight="700"
        fill="currentColor"
        font-family="-apple-system, system-ui, sans-serif">Z</text
      >
      <g
        fill="none"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M12.5 3.5v8" />
        <path d="M10.5 9.5 12.5 11.5 14.5 9.5" />
      </g>
    </svg>
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

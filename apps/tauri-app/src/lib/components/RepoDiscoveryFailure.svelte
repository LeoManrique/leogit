<script lang="ts">
  import { discoveringRepos, discoveryError, rediscoverRepos } from '$lib/services/repoDiscovery'
  import Icon from './Icon.svelte'

  /*
    The discovery walk failed. One inline row above the list, never a phase
    swap: whatever a previous pass found is still listed below and still
    openable, and replacing the list with an error screen would take the
    repositories away along with the bad news. The native
    `RepoPickerList.discoveryFailure` is the same row.

    Shared by both pickers, like the empty state and the sort toggle beside it —
    the two lists are one component family (STYLE, *Repo pickers*).
  */
</script>

{#if $discoveryError}
  <div class="discovery-failure" title={$discoveryError}>
    <Icon name="exclamationmark-triangle-fill" size={13} />
    <span>Couldn't search for repositories.</span>
    <!-- A second click would coalesce into the running pass and look like
         nothing happened; the disabled state is the feedback. -->
    <button type="button" onclick={() => void rediscoverRepos()} disabled={$discoveringRepos}>
      Retry
    </button>
  </div>
{/if}

<style>
  .discovery-failure {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-inactive);
  }

  /* `:global` because the glyph is a child component's element now — Svelte
     stamps its scope hash only on elements in this template. Size and
     `flex-shrink` moved to the `size` prop and to `Icon` itself, leaving this
     rule doing the one thing it alone can: the tint. */
  .discovery-failure :global(svg) {
    color: var(--status-yellow);
  }

  .discovery-failure span {
    flex: 1;
    min-width: 0;
  }

  .discovery-failure button {
    padding: 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--border-active);
    cursor: pointer;
  }

  .discovery-failure button:hover:not(:disabled) {
    text-decoration: underline;
  }

  .discovery-failure button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>

<script lang="ts">
  import { discoveringRepos, discoveryError, rediscoverRepos } from '$lib/services/repoDiscovery'

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
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path
        d="M8 1.5a.9.9 0 0 1 .78.45l6.1 10.85A.9.9 0 0 1 14.1 14H1.9a.9.9 0 0 1-.78-1.2L7.22 1.95A.9.9 0 0 1 8 1.5Zm0 3.75a.7.7 0 0 0-.7.75l.2 3.1a.5.5 0 0 0 1 0l.2-3.1a.7.7 0 0 0-.7-.75Zm0 5.5a.85.85 0 1 0 0 1.7.85.85 0 0 0 0-1.7Z"
      />
    </svg>
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

  .discovery-failure svg {
    width: 13px;
    height: 13px;
    flex-shrink: 0;
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

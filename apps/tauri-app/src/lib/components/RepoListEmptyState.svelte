<script lang="ts">
  /*
    What a repo list says when it has no rows to show.

    Three distinguishable answers, because "No repositories" for all of them
    told the user nothing they could act on: still looking, found none anywhere
    (here is where we looked, and here is how to change that), or found some but
    none match what you typed. The dropdown used to collapse all three into the
    first string while the startup picker had the rich version — the same
    question answered two different ways in one app.
  */
  interface Props {
    /** A discovery walk is running. Only shown when there are no rows yet:
     *  later refreshes replace rows in place rather than blinking a spinner. */
    discovering: boolean
    /** Whether discovery found anything at all — what separates "none found"
     *  from "none matched your filter". */
    hasRepos: boolean
    /** The folders discovery actually walked, named so "found nothing" becomes
     *  something the user can act on (usually "that's not where my code is"). */
    scannedPaths: string[]
    /** Opens Settings, where the scan paths are edited. */
    onOpenSettings: () => void
  }

  let { discovering, hasRepos, scannedPaths, onOpenSettings }: Props = $props()
</script>

<div class="empty">
  {#if discovering}
    <div class="spinner"></div>
    <p class="title">Looking for repositories…</p>
  {:else}
    {#if hasRepos}
      <p class="title">No matching repositories</p>
    {:else}
      <p class="title">No repositories found</p>
      {#if scannedPaths.length > 0}
        <p class="detail">Searched these folders:</p>
        <ul class="paths">
          {#each scannedPaths as path (path)}
            <li>{path}</li>
          {/each}
        </ul>
      {/if}
    {/if}
    <!-- The call to action belongs to both dead ends, not just the empty one.
         "None matched" is what a user sees when the repo they are looking for
         lives somewhere discovery was never pointed at — which is the same
         problem, reached by typing its name. -->
    <button class="action" onclick={onOpenSettings}>Choose Folders to Search</button>
  {/if}
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-faint);
    padding: 24px 16px;
    text-align: center;
    font-size: 12px;
  }

  .title {
    margin: 0;
  }

  .detail {
    margin: 0;
    font-size: 11px;
  }

  /* Mono because these are paths, and seeing them is what turns "found
     nothing" into something the user can act on. */
  .paths {
    margin: 0;
    padding: 0;
    list-style: none;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    max-height: 120px;
    overflow-y: auto;
  }

  .paths li {
    line-height: 1.6;
    word-break: break-all;
  }

  .action {
    margin-top: 4px;
    padding: 5px 12px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    transition: background 100ms ease;
  }

  .action:hover {
    background: var(--surface-hover);
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--border-inactive);
    border-top-color: var(--border-active);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>

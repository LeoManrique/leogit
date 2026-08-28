<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    /**
     * The load has outlived the slow threshold. Until it does, the swap is
     * silent — most reads land in well under it, and a spinner that flashes
     * for 40 ms is noise, not feedback (FRONTEND §6.3).
     */
    stale?: boolean
    children: Snippet
  }

  let { stale = false, children }: Props = $props()
</script>

<!--
  The diff pane's loading treatment, shared by the changes tab and the commit
  detail because they are the same pane twice and had already been written
  twice.

  What is on screen stays on screen. Crossing the slow threshold dims it and
  lays a spinner over it; it never unmounts. The old shape swapped the whole
  pane for a "Loading diff…" line, which threw away the rendered rows, their
  syntax tokens and the user's scroll position for every load — including the
  ones that came back identical. GitHub Desktop's `SeamlessDiffSwitcher` does
  it this way, and the native pane is meant to (its own comment says so).

  Dimming is what marks the content as no longer current: it is still the
  truth about the file the user was looking at, so it stays readable, and the
  spinner over it says a newer answer is coming.
-->
<div class="diff-pane">
  <div class="diff-pane-content" class:stale>
    {@render children()}
  </div>
  {#if stale}
    <div class="diff-pane-progress" role="status" aria-label="Loading diff">
      <div class="spinner"></div>
    </div>
  {/if}
</div>

<style>
  .diff-pane {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .diff-pane-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    transition: opacity 120ms ease;
  }

  .diff-pane-content.stale {
    opacity: 0.45;
  }

  /*
    Pinned near the top rather than centred: the rows underneath are what the
    user is reading, and a spinner in the middle of them sits on the line they
    were looking at. The pane can also be tall enough that a centred spinner
    is below the fold.
  */
  .diff-pane-progress {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    padding-top: 48px;
    pointer-events: none;
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

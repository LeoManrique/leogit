<script lang="ts">
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    /** The branch whose commits are being brought in. */
    source: string
    /** The branch they land on — always the checked-out one. */
    target: string
    /**
     * How many commits the merge would bring in, or null while the count is
     * still being read. Null renders no line at all rather than a "0" that
     * would then have to be taken back.
     */
    commitCount: number | null
    isMerging: boolean
    onMerge: () => void
    onSquashMerge: () => void
    onCancel: () => void
  }

  let { source, target, commitCount, isMerging, onMerge, onSquashMerge, onCancel }: Props = $props()

  /*
    Nothing to bring in. GitHub Desktop says so and disables its primary rather
    than offering a merge that would be a no-op; the native client used to
    print "Brings in 0 commits." beside a live Merge button, which invites a
    click that does nothing and then reports success.
  */
  const upToDate = $derived(commitCount === 0)

  function escape(): void {
    if (!isMerging) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isMerging) onCancel()
  }}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    use:autofocus
    use:dismissOnEscape={escape}
  >
    <div class="modal-header">
      <h2>Merge Branch</h2>
    </div>
    <div class="modal-body">
      <p>
        Merge <code>{source}</code> into <code>{target}</code>.
      </p>
      {#if upToDate}
        <p class="muted">
          <code>{target}</code> already contains everything on <code>{source}</code>. There is
          nothing to merge.
        </p>
      {:else if commitCount !== null}
        <p class="muted">
          Brings in {commitCount}
          {commitCount === 1 ? 'commit' : 'commits'}.
        </p>
      {/if}
      <p class="muted">
        <strong>Squash &amp; Merge</strong> replaces them with a single commit on
        <code>{target}</code>.
      </p>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isMerging}>Cancel</button>
      <button class="btn-secondary" onclick={onSquashMerge} disabled={isMerging || upToDate}>
        Squash &amp; Merge
      </button>
      <button class="btn-primary" onclick={onMerge} disabled={isMerging || upToDate}>
        {isMerging ? 'Merging…' : 'Merge'}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1500;
  }

  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 420px;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .modal:focus {
    outline: none;
  }

  .modal-header {
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .modal-body {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .modal-body p {
    margin: 0;
    font-size: 13px;
    color: var(--text-primary);
  }

  .modal-body .muted {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .modal-body code {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    background: transparent;
  }

  .modal-body strong {
    font-weight: 600;
    color: var(--text-primary);
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary,
  .btn-primary {
    padding: 3px 14px;
    font-size: 12px;
    font-weight: 500;
    border-radius: 6px;
    cursor: pointer;
    border: 1px solid var(--border-strong);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-family: inherit;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn-primary {
    background: var(--border-active);
    border-color: var(--border-active);
    color: var(--on-accent);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-secondary:disabled,
  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

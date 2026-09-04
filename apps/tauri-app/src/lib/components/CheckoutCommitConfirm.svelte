<script lang="ts">
  import type { CommitInfo } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    /** The commit being checked out — shown for context in the warning. */
    commit: CommitInfo
    isCheckingOut: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let { commit, isCheckingOut, onConfirm, onCancel }: Props = $props()

  function escape(): void {
    if (!isCheckingOut) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onCancel()
  }}
>
  <!-- Focused on mount so Tab starts inside the dialog rather than behind it. -->
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    use:autofocus
    use:dismissOnEscape={escape}
  >
    <div class="modal-header">
      <h2>Checkout commit?</h2>
    </div>
    <div class="modal-body">
      <p class="target">
        <code class="sha">{commit.short_sha}</code>
        <span class="summary">{commit.summary}</span>
      </p>
      <p>
        Checking out a commit creates a <strong>detached HEAD</strong> — you'll no longer be on
        any branch. New commits won't belong to a branch until you create one.
      </p>
      <p class="muted">
        To return, pick a branch from the branch menu. Your branches are unchanged.
      </p>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isCheckingOut}>Cancel</button>
      <button class="btn-primary" onclick={onConfirm} disabled={isCheckingOut}>
        {isCheckingOut ? 'Checking out…' : 'Checkout'}
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

  .modal-body strong {
    font-weight: 600;
  }

  /* Commit being checked out: monospace SHA pill + truncated summary. */
  .target {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .sha {
    flex: 0 0 auto;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--badge-fg);
    background: var(--badge-bg);
    padding: 1px 6px;
    border-radius: 5px;
  }

  .summary {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    min-width: 0;
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
    color: var(--on-accent);
    border-color: var(--border-active);
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
  /* Focused on mount for Escape; the ring would be chrome nobody asked for. */
  .modal:focus {
    outline: none;
  }
</style>

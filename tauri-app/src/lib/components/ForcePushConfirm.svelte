<script lang="ts">
  interface Props {
    branch: string
    remote: string
    isPushing: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let { branch, remote, isPushing, onConfirm, onCancel }: Props = $props()

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isPushing) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onCancel()
  }}
  onkeydown={handleKeyDown}
>
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
    <div class="modal-header">
      <h2>Force push with lease</h2>
    </div>
    <div class="modal-body">
      <p>
        This will overwrite <code>{remote}/{branch}</code> with your local branch.
      </p>
      <p class="muted">
        <code>--force-with-lease</code> refuses the push if someone else has pushed since
        your last fetch, so it's safer than <code>--force</code>. It cannot be undone if it
        succeeds.
      </p>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isPushing}>Cancel</button>
      <button class="btn-danger" onclick={onConfirm} disabled={isPushing}>
        {isPushing ? 'Force-pushing…' : 'Force push'}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
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

  .modal-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--status-red);
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
    color: var(--text-primary);
    background: transparent;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary,
  .btn-danger {
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

  .btn-danger {
    color: var(--status-red);
  }

  .btn-danger:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn-secondary:disabled,
  .btn-danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

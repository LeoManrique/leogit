<script lang="ts">
  interface Props {
    title?: string
    message: string
    onDismiss: () => void
    onRetry?: () => void
  }

  let { title = 'Error', message, onDismiss, onRetry }: Props = $props()

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') onDismiss()
  }
</script>

<div class="overlay" role="presentation" onclick={onDismiss} onkeydown={handleKeyDown}>
  <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <h2>{title}</h2>
      <button class="close-btn" onclick={onDismiss} aria-label="Close">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
          <line x1="4" y1="4" x2="12" y2="12" />
          <line x1="12" y1="4" x2="4" y2="12" />
        </svg>
      </button>
    </div>
    <div class="modal-body">
      <pre class="message">{message}</pre>
    </div>
    <div class="modal-footer">
      {#if onRetry}
        <button class="btn-secondary" onclick={onRetry}>Retry</button>
      {/if}
      <button class="btn-primary" onclick={onDismiss}>Dismiss</button>
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
    z-index: 2000;
  }

  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 420px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--status-red);
  }

  .close-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 4px;
  }

  .close-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .modal-body {
    flex: 1;
    padding: 16px;
    overflow-y: auto;
  }

  .message {
    margin: 0;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-wrap: break-word;
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
    border-radius: 6px;
    cursor: pointer;
    font-weight: 500;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
  }

  .btn-secondary:hover {
    background: var(--surface-hover);
  }

  .btn-primary {
    background: var(--border-active);
    color: #ffffff;
    border: 1px solid var(--border-active);
  }

  .btn-primary:hover {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }
</style>

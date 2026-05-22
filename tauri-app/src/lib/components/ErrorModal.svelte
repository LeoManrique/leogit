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
      <button class="close-btn" onclick={onDismiss} aria-label="Close">✕</button>
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
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
  }

  .modal {
    background: var(--bg-primary);
    border: 1px solid var(--status-red);
    border-radius: 6px;
    width: 90%;
    max-width: 560px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
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
    font-size: 15px;
    font-weight: 600;
    color: var(--status-red);
  }

  .close-btn {
    padding: 4px 8px;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    cursor: pointer;
    border-radius: 3px;
  }

  .modal-body {
    flex: 1;
    padding: 16px;
    overflow-y: auto;
  }

  .message {
    margin: 0;
    padding: 12px;
    background: var(--bg-secondary);
    border-radius: 4px;
    color: var(--text-primary);
    font-family: 'Monaco', 'Menlo', monospace;
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
    padding: 6px 14px;
    font-size: 12px;
    border-radius: 4px;
    cursor: pointer;
  }

  .btn-secondary {
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
  }

  .btn-primary {
    background: var(--status-red);
    color: white;
    border: 1px solid var(--status-red);
  }
</style>

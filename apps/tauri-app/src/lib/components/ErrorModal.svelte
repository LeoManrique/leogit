<script lang="ts">
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import Icon from './Icon.svelte'

  interface Props {
    title?: string
    message: string
    onDismiss: () => void
    onRetry?: () => void
  }

  let { title = 'Error', message, onDismiss, onRetry }: Props = $props()

</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onDismiss()
  }}
>
  <!-- Focused on mount so Tab starts inside the dialog rather than behind it. -->
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    use:autofocus
    use:dismissOnEscape={onDismiss}
  >
    <div class="modal-header">
      <h2>{title}</h2>
      <button class="close-btn" onclick={onDismiss} aria-label="Close">
        <Icon name="xmark" size={11} weight="semibold" />
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
    background: var(--overlay-backdrop);
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

  /* Size and weight come from app.css; the error title only takes the red. */
  .modal-header h2 {
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
    color: var(--on-accent);
    border: 1px solid var(--border-active);
  }

  .btn-primary:hover {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }
  /* Focused on mount for Escape; the ring would be chrome nobody asked for. */
  .modal:focus {
    outline: none;
  }
</style>

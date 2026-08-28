<script lang="ts">
  import type { Snippet } from 'svelte'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    title: string
    /** The dialog's body: one or two short paragraphs naming the real outcome. */
    body: Snippet
    /** Verb on the confirming button — "Delete", "Abort merge". */
    confirmLabel: string
    /** Replaces `confirmLabel` while the operation runs — "Deleting…". */
    busyLabel: string
    /**
     * True while the confirmed operation is in flight. The dialog stays up with
     * its buttons locked rather than vanishing on the click: the operation is
     * one the user is waiting on, and a dialog that disappears leaving nothing
     * behind reads as though nothing happened (HI-9's ruling, applied here).
     */
    isBusy: boolean
    /**
     * A dialog whose confirmation loses work gets the red button and refuses a
     * backdrop dismiss — declining has to be deliberate (STYLE.md, *Modals /
     * dialogs*). Escape still works, because Escape is a deliberate keypress.
     */
    destructive?: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let {
    title,
    body,
    confirmLabel,
    busyLabel,
    isBusy,
    destructive = false,
    onConfirm,
    onCancel,
  }: Props = $props()

  // Escape is refused, not passed on, while the operation runs: the dialog is
  // still the frontmost thing and hiding it wouldn't stop the work.
  function escape(): void {
    if (!isBusy) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isBusy && !destructive) onCancel()
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
      <h2 class:destructive>{title}</h2>
    </div>
    <div class="modal-body">
      {@render body()}
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isBusy}>Cancel</button>
      <button
        class={destructive ? 'btn-danger' : 'btn-primary'}
        onclick={onConfirm}
        disabled={isBusy}
      >
        {isBusy ? busyLabel : confirmLabel}
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

  .modal:focus {
    outline: none;
  }

  .modal-header {
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .modal-header h2.destructive {
    color: var(--status-red);
  }

  .modal-body {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .modal-body :global(p) {
    margin: 0;
    font-size: 13px;
    color: var(--text-primary);
  }

  .modal-body :global(p.muted) {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .modal-body :global(code) {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
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
  .btn-primary,
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

  .btn-secondary:hover:not(:disabled),
  .btn-danger:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn-primary {
    background: var(--border-active);
    border-color: var(--border-active);
    color: #ffffff;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-danger {
    color: var(--status-red);
  }

  .btn-secondary:disabled,
  .btn-primary:disabled,
  .btn-danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

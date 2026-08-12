<script lang="ts">
  import type { FileEntry } from '$lib/api/commands'

  interface Props {
    files: FileEntry[]
    isDiscarding: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let { files, isDiscarding, onConfirm, onCancel }: Props = $props()

  const single = $derived(files.length === 1 ? files[0] : null)

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isDiscarding) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isDiscarding) onCancel()
  }}
  onkeydown={handleKeyDown}
>
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
    <div class="modal-header">
      <h2>Discard changes</h2>
    </div>
    <div class="modal-body">
      {#if single}
        <p>
          Are you sure you want to discard all changes to <code>{single.path}</code>?
        </p>
      {:else}
        <p>
          Are you sure you want to discard all changes to
          <strong>{files.length}</strong> selected files?
        </p>
      {/if}
      <p class="muted">
        Tracked files are reverted to the last commit. New (untracked) files are moved to the
        Trash, so they can be recovered.
      </p>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isDiscarding}>Cancel</button>
      <button class="btn-danger" onclick={onConfirm} disabled={isDiscarding}>
        {isDiscarding ? 'Discarding…' : 'Discard changes'}
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
    word-break: break-word;
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

<script lang="ts">
  import type { DiscardPlan, FileEntry } from '$lib/api/commands'

  interface Props {
    files: FileEntry[]
    /**
     * What the discard would actually do, per path, as core decides it — the
     * same decision the action runs on. Null until the answer arrives, which
     * is one round trip after the dialog opens.
     */
    plan: DiscardPlan | null
    isDiscarding: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let { files, plan, isDiscarding, onConfirm, onCancel }: Props = $props()

  const single = $derived(files.length === 1 ? files[0] : null)

  /*
    The two outcomes are not interchangeable — one is undone by committing
    again, the other sends a file to the Trash — and which one a row gets is
    not visible from its status letter: a staged re-add of a path that exists
    in HEAD is restorable, a rename whose original is not in HEAD is not, and
    under an unborn HEAD nothing is. So the dialog names the real outcome
    instead of reciting both rules and leaving the user to guess which applies.
  */
  const outcome = $derived.by(() => {
    if (!plan) return null
    const restored = plan.restore.length
    const trashed = plan.trash.length
    if (restored > 0 && trashed > 0) {
      return `${restored} ${restored === 1 ? 'file goes' : 'files go'} back to the last commit; ${trashed} ${trashed === 1 ? 'moves' : 'move'} to the Trash.`
    }
    if (restored > 0) {
      return restored === 1
        ? 'It goes back to its committed state.'
        : `All ${restored} go back to their committed state.`
    }
    if (trashed > 0) {
      return trashed === 1
        ? 'It was never committed, so there is nothing to restore it to — it moves to the Trash instead.'
        : `None of the ${trashed} were ever committed, so they move to the Trash rather than being restored.`
    }
    return 'There is nothing to discard.'
  })

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
        {outcome ?? 'Working out what this will do…'}
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

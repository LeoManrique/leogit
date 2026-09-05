<script lang="ts">
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import { basename } from '$lib/utils/path'
  import Icon from './Icon.svelte'

  interface Props {
    /** Absolute path of the folder `leogit <dir>` pointed at. */
    path: string
    isInitializing: boolean
    /** Message from a failed `git init`; cleared on the next attempt. */
    error: string
    onConfirm: () => void
    onCancel: () => void
  }

  let { path, isInitializing, error, onConfirm, onCancel }: Props = $props()

  const folderName = $derived(basename(path))

  // Enter is left to the autofocused primary button's native activation, so it
  // can't fire the confirm twice.
  function escape(): void {
    if (!isInitializing) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isInitializing) onCancel()
  }}
>
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1" use:dismissOnEscape={escape}>
    <div class="modal-header">
      <h2>Create a repository here?</h2>
      <button class="close-btn" onclick={onCancel} disabled={isInitializing} aria-label="Close">
        <Icon name="xmark" size={14} weight="medium" />
      </button>
    </div>
    <div class="modal-body">
      <div class="target">
        <span class="folder">{folderName}</span>
        <span class="path">{path}</span>
      </div>
      <p>
        This folder isn’t a Git repository yet. Creating one leaves your files exactly where they
        are — nothing is committed until you commit it.
      </p>
      {#if error}
        <div class="init-error">{error}</div>
      {/if}
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isInitializing}>Cancel</button>
      <button class="btn-primary" onclick={onConfirm} disabled={isInitializing} use:autofocus>
        {isInitializing ? 'Creating…' : 'Create Repository'}
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .close-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .close-btn:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .target {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .folder {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    word-break: break-all;
  }

  .modal-body p {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }

  .init-error {
    font-size: 12px;
    color: var(--status-red);
    word-break: break-word;
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
    font-family: inherit;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
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

  .btn-primary:disabled,
  .btn-secondary:disabled,
  .close-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

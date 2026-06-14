<script lang="ts">
  import { untrack } from 'svelte'
  import { autofocus } from '$lib/actions/autofocus'

  interface Props {
    defaultName: string
    isPublishing: boolean
    onPublish: (name: string, description: string, isPrivate: boolean) => void
    onCancel: () => void
  }

  let { defaultName, isPublishing, onPublish, onCancel }: Props = $props()

  // Seed the editable name from the prop once at mount (the dialog is re-created
  // each time it opens). `untrack` documents that the one-time read is intended.
  let name = $state(untrack(() => defaultName))
  let description = $state('')
  let isPrivate = $state(true)

  const canPublish = $derived(name.trim().length > 0 && !isPublishing)

  function submit() {
    if (!canPublish) return
    onPublish(name.trim(), description.trim(), isPrivate)
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isPublishing) onCancel()
    if (e.key === 'Enter' && canPublish) submit()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isPublishing) onCancel()
  }}
  onkeydown={handleKeyDown}
>
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
    <div class="modal-header">
      <h2>Publish repository</h2>
      <button class="close-btn" onclick={onCancel} disabled={isPublishing} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
          <line x1="4" y1="4" x2="12" y2="12" />
          <line x1="12" y1="4" x2="4" y2="12" />
        </svg>
      </button>
    </div>
    <div class="modal-body">
      <label class="field">
        <span class="field-label">Name</span>
        <input
          class="text-input"
          type="text"
          bind:value={name}
          disabled={isPublishing}
          spellcheck="false"
          autocapitalize="off"
          autocorrect="off"
          use:autofocus
        />
      </label>
      <label class="field">
        <span class="field-label">Description</span>
        <input
          class="text-input"
          type="text"
          bind:value={description}
          disabled={isPublishing}
        />
      </label>
      <label class="checkbox-row">
        <input type="checkbox" bind:checked={isPrivate} disabled={isPublishing} />
        <span>Keep this code private</span>
      </label>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isPublishing}>Cancel</button>
      <button class="btn-primary" onclick={submit} disabled={!canPublish}>
        {isPublishing ? 'Publishing…' : 'Publish repository'}
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
    max-width: 440px;
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

  .modal-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
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
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .text-input {
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    font-size: 13px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .text-input:focus {
    outline: none;
    border-color: var(--border-active);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-primary);
    cursor: pointer;
  }

  .checkbox-row input {
    cursor: pointer;
    accent-color: var(--border-active);
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
    color: #ffffff;
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

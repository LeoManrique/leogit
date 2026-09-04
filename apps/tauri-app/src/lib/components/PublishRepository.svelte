<script lang="ts">
  import { untrack } from 'svelte'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import Icon from './Icon.svelte'

  interface Props {
    defaultName: string
    isPublishing: boolean
    /**
     * `gh`'s own failure text, stated here with every field intact. The common
     * one is a name already taken, whose fix is a character in the field behind
     * the dialog — a modal stacked over it made that two dismissals and a
     * retype, and the dialog underneath was still holding the same doomed name.
     */
    error?: string
    onPublish: (name: string, description: string, isPrivate: boolean) => void
    onCancel: () => void
  }

  let { defaultName, isPublishing, error, onPublish, onCancel }: Props = $props()

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

  // Enter submits from anywhere in the dialog; Escape belongs to the overlay
  // stack, which is what decides that this dialog — and not the popover it may
  // be standing on — is the one being dismissed.
  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && canPublish) submit()
  }

  function escape(): void {
    if (!isPublishing) onCancel()
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
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1" use:dismissOnEscape={escape}>
    <div class="modal-header">
      <h2>Publish repository</h2>
      <button class="close-btn" onclick={onCancel} disabled={isPublishing} aria-label="Close">
        <Icon name="xmark" size={14} weight="medium" />
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
      <p class="hint">
        Publishes to github.com under your <code>gh</code> account — use
        <code>owner/name</code> to target an organization.
      </p>
      {#if isPublishing}
        <!-- `gh repo create` streams nothing parseable, so this is the honest
             shape: motion that says work is happening, and no number it would
             have to invent. -->
        <div class="progress" role="status" aria-label="Publishing">
          <div class="progress-fill"></div>
        </div>
      {/if}
      {#if error}
        <p class="error">{error}</p>
      {/if}
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

  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-muted);
  }

  .hint code {
    font-family: var(--font-mono);
    font-size: 11px;
  }

  /* The Clone dialog's 4px bar, running indeterminate: same shape, no number. */
  .progress {
    height: 4px;
    border-radius: 2px;
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    width: 40%;
    border-radius: 2px;
    background: var(--border-active);
    animation: sweep 1.2s ease-in-out infinite;
  }

  @keyframes sweep {
    from {
      transform: translateX(-110%);
    }
    to {
      transform: translateX(260%);
    }
  }

  /* gh's own text: mono, selectable, wrapped — the failure usually names the
     repository that already exists, which is worth copying. */
  .error {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--status-red);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    max-height: 120px;
    overflow-y: auto;
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

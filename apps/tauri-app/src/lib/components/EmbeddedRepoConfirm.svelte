<script lang="ts">
  import type { FileEntry } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    /** Embedded-repo entries included in this commit. */
    repos: FileEntry[]
    /** Name of the outer repo the gitlink lands in (for the warning copy). */
    outerRepo: string
    isCommitting: boolean
    onConfirm: () => void
    onCancel: () => void
  }

  let { repos, outerRepo, isCommitting, onConfirm, onCancel }: Props = $props()

  const many = $derived(repos.length > 1)

  // Every dismissal is the same decision, so they answer to one guard: the
  // commit this dialog is a pause inside cannot be called off once it starts,
  // and a backdrop click that closed the dialog anyway left the composer live
  // over a commit still running underneath it.
  const canCancel = $derived(!isCommitting)

  function escape(): void {
    if (canCancel) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && canCancel) onCancel()
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
      <h2>Commit nested {many ? 'repositories' : 'repository'} as a link?</h2>
    </div>
    <div class="modal-body">
      <ul class="repo-list">
        {#each repos as repo (repo.path)}
          <li><code>{repo.display_name}</code></li>
        {/each}
      </ul>
      <p>
        {many ? 'These folders are their own Git repositories' : 'This folder is its own Git repository'}.
        {many ? 'They’ll' : 'It’ll'} be committed as a
        <strong>link</strong> to the current commit — the {many ? 'folders’' : 'folder’s'} files
        won’t be copied into <code>{outerRepo}</code>.
      </p>
      <p class="muted">
        Anyone cloning <code>{outerRepo}</code> won’t get {many ? 'those files' : 'those files'} unless
        {many ? 'each is' : 'it’s'} set up as a submodule.
      </p>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={!canCancel}>Cancel</button>
      <button class="btn-primary" onclick={onConfirm} disabled={isCommitting}>
        {isCommitting ? 'Committing…' : 'Commit as link'}
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
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-inactive);
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

  .modal-body strong {
    font-weight: 600;
  }

  .repo-list {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .repo-list li {
    font-size: 13px;
    color: var(--text-primary);
  }

  .modal-body code,
  .repo-list code {
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
  .btn-primary {
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

  .btn-primary {
    background: var(--border-active);
    color: var(--on-accent);
    border-color: var(--border-active);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-secondary:disabled,
  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  /* Focused on mount for Escape; the ring would be chrome nobody asked for. */
  .modal:focus {
    outline: none;
  }
</style>

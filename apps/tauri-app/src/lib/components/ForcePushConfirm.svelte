<script lang="ts">
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    /**
     * Where the push would land, named from git's own tracking configuration
     * (`RepoStatus.upstream`) rather than composed from the remote and the
     * local branch name — those differ whenever the upstream branch is named
     * something else, and the dialog would then promise to overwrite a branch
     * git was never going to touch.
     */
    upstream: string
    isPushing: boolean
    /** A refused push, stated here rather than in a modal over this dialog: a
     *  stale lease is answered by fetching and pressing the same button again,
     *  which is one dismissal away instead of two. */
    error?: string
    onConfirm: () => void
    onCancel: () => void
  }

  let { upstream, isPushing, error, onConfirm, onCancel }: Props = $props()

  function escape(): void {
    if (!isPushing) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onCancel()
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
      <h2>Force push with lease</h2>
    </div>
    <div class="modal-body">
      <p>
        This will overwrite <code>{upstream}</code> with your local branch.
      </p>
      <p class="muted">
        <code>--force-with-lease</code> refuses the push if someone else has pushed since
        your last fetch, so it's safer than <code>--force</code>. It cannot be undone if it
        succeeds.
      </p>
      {#if error}
        <p class="error">{error}</p>
      {/if}
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isPushing}>Cancel</button>
      <button class="btn-danger" onclick={onConfirm} disabled={isPushing}>
        {isPushing ? 'Force-pushing…' : 'Force push'}
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

  /* Size and weight come from app.css; this dialog only recolours its title. */
  .modal-header h2 {
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

  /* Git's own rejection text, kept selectable and wrapped — it names the ref
     that moved, which is the part worth copying into a fetch. */
  .modal-body .error {
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
  /* Focused on mount for Escape; the ring would be chrome nobody asked for. */
  .modal:focus {
    outline: none;
  }
</style>

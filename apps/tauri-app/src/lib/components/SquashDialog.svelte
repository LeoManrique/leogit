<script lang="ts">
  import { untrack } from 'svelte'
  import type { SquashDraft } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    /** How many commits become one. */
    count: number
    /** What the fields open with — core's reading of the selected commits. */
    draft: SquashDraft
    /**
     * The upstream the selected commits are already on, or null when none of
     * them is pushed. Named from `RepoStatus.upstream`, as the force-push
     * confirmation names it — and empty when core says they are pushed before
     * a status read has named the upstream, which still gets the warning.
     */
    pushedTo: string | null
    isSquashing: boolean
    /** Another write holds the window's slot: the reason, beside a disabled button. */
    blocked?: string
    /**
     * A squash core refused or undid, stated here with the message intact — the
     * branch is back where it began, and the fix (a hook, a signer) is usually
     * outside the app, after which the same button is pressed again.
     */
    error?: string
    onSquash: (summary: string, description: string) => void
    onCancel: () => void
  }

  let { count, draft, pushedTo, isSquashing, blocked, error, onSquash, onCancel }: Props =
    $props()

  // Seeded once at mount (the dialog is re-created each time it opens); after
  // that the fields are the user's.
  let summary = $state(untrack(() => draft.summary))
  let description = $state(untrack(() => draft.description))

  const canSquash = $derived(summary.trim().length > 0 && !isSquashing && blocked === undefined)

  function submit(): void {
    if (!canSquash) return
    onSquash(summary.trim(), description.trim())
  }

  // The composer's chord: Return belongs to the description, so ⌘/Ctrl+Return
  // is what submits, from either field.
  function handleKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      submit()
    }
  }

  function escape(): void {
    if (!isSquashing) onCancel()
  }
</script>

<div
  class="overlay"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget && !isSquashing) onCancel()
  }}
  onkeydown={handleKeyDown}
>
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1" use:dismissOnEscape={escape}>
    <div class="modal-header">
      <h2>Squash {count} Commits</h2>
    </div>
    <div class="modal-body">
      <input
        class="text-input"
        type="text"
        placeholder="Summary (required)"
        aria-label="Summary"
        bind:value={summary}
        disabled={isSquashing}
        use:autofocus
      />
      <textarea
        class="text-input description"
        placeholder="Description"
        aria-label="Description"
        bind:value={description}
        disabled={isSquashing}
      ></textarea>
      {#if draft.co_authors.length > 0}
        <p class="muted">
          Co-authored by {draft.co_authors.join(', ')}.
        </p>
      {/if}
      {#if pushedTo !== null}
        <p class="muted">
          {#if pushedTo}
            These commits are already on <code>{pushedTo}</code>.
          {:else}
            These commits are already pushed.
          {/if}
          After squashing, the next push is a force push.
        </p>
      {/if}
      {#if blocked}
        <p class="muted">{blocked}</p>
      {/if}
      {#if error}
        <p class="error">{error}</p>
      {/if}
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={onCancel} disabled={isSquashing}>Cancel</button>
      <button class="btn-primary" onclick={submit} disabled={!canSquash}>
        {isSquashing ? 'Squashing…' : `Squash ${count} Commits`}
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
    max-width: 480px;
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

  /* Tall enough for a handful of folded messages, and the user's to resize —
     downwards only, so the dialog keeps its width. */
  .description {
    min-height: 140px;
    max-height: 40vh;
    resize: vertical;
    line-height: 1.4;
  }

  .muted {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .muted code {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
  }

  /* Git's own text: mono, selectable, wrapped, and scrolled before it can push
     the buttons out of the dialog. */
  .error {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--status-red);
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-user-select: text;
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
  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Focus goes to the summary field; the dialog itself never shows a ring. */
  .modal:focus {
    outline: none;
  }
</style>

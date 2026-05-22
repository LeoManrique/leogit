<script lang="ts">
  interface Props {
    sourceBranch: string
    targetBranch: string
    onMerge: () => void
    onSquashMerge: () => void
    onAbort: () => void
  }

  let { sourceBranch = '', targetBranch = '', onMerge, onSquashMerge, onAbort }: Props = $props()
</script>

<div class="merge-overlay" role="dialog" aria-modal="true">
  <div class="merge-modal">
    <h2 class="modal-header">Merge Branch</h2>

    <div class="merge-content">
      <p class="merge-description">
        Merge <code class="branch-name">{sourceBranch}</code> into
        <code class="branch-name">{targetBranch}</code>
      </p>

      <div class="merge-options">
        <div class="option-info">
          <p class="option-title">Choose merge strategy:</p>
        </div>
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-cancel" onclick={onAbort}>Cancel</button>
      <button class="btn-squash" onclick={onSquashMerge}>Squash & Merge</button>
      <button class="btn-merge" onclick={onMerge}>Merge</button>
    </div>
  </div>
</div>

<style>
  .merge-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1001;
  }

  .merge-modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 420px;
    box-shadow: var(--shadow-popover);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .modal-header {
    padding: 14px 16px 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-inactive);
    margin: 0;
  }

  .merge-content {
    padding: 16px;
  }

  .merge-description {
    margin: 0 0 12px 0;
    font-size: 13px;
    color: var(--text-primary);
  }

  .branch-name {
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 0;
    font-weight: 500;
  }

  .merge-options {
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
  }

  .option-info {
    margin: 0;
  }

  .option-title {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .modal-footer {
    display: flex;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
    justify-content: flex-end;
  }

  .btn-merge,
  .btn-squash,
  .btn-cancel {
    padding: 3px 14px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    transition: background 120ms ease, border-color 120ms ease;
  }

  .btn-merge {
    background: var(--border-active);
    color: #ffffff;
    border-color: var(--border-active);
  }

  .btn-merge:hover {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-squash {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-squash:hover {
    background: var(--surface-hover);
  }

  .btn-cancel {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-cancel:hover {
    background: var(--surface-hover);
  }
</style>

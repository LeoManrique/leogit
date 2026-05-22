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
      <button class="btn-merge" onclick={onMerge}>Merge</button>
      <button class="btn-squash" onclick={onSquashMerge}>Squash & Merge</button>
      <button class="btn-cancel" onclick={onAbort}>Cancel</button>
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
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1001;
  }

  .merge-modal {
    background: var(--bg-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 8px;
    width: 90%;
    max-width: 500px;
    box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 10px 10px -5px rgba(0, 0, 0, 0.04);
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    padding: 16px 20px;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-inactive);
    margin: 0;
  }

  .merge-content {
    padding: 20px;
  }

  .merge-description {
    margin: 0 0 16px 0;
    font-size: 14px;
    color: var(--text-primary);
  }

  .branch-name {
    background: transparent;
    color: var(--status-blue);
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', 'Courier New', monospace;
    padding: 2px 4px;
    border-radius: 3px;
  }

  .merge-options {
    padding: 12px;
    background: var(--bg-secondary);
    border-radius: 4px;
  }

  .option-info {
    margin: 0;
  }

  .option-title {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .modal-footer {
    display: flex;
    gap: 8px;
    padding: 16px 20px;
    border-top: 1px solid var(--border-inactive);
    justify-content: flex-end;
  }

  .btn-merge,
  .btn-squash,
  .btn-cancel {
    padding: 8px 16px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: all 150ms ease;
  }

  .btn-merge {
    background: var(--status-green);
    color: white;
  }

  .btn-merge:hover {
    opacity: 0.9;
  }

  .btn-squash {
    background: var(--status-blue);
    color: white;
  }

  .btn-squash:hover {
    opacity: 0.9;
  }

  .btn-cancel {
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
  }

  .btn-cancel:hover {
    background: var(--bg-tertiary);
  }
</style>

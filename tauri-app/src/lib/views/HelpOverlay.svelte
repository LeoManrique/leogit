<script lang="ts">
  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  function handleOverlayKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }

  const shortcuts = [
    { key: 'Ctrl/Cmd + Enter', description: 'Commit selected files' },
    { key: 'Ctrl/Cmd + G', description: 'Generate commit message with AI' },
    { key: 'Ctrl/Cmd + P', description: 'Cycle AI provider' },
    { key: 'Ctrl/Cmd + R', description: 'Refresh status' },
    { key: 'Ctrl/Cmd + L', description: 'Toggle Changes / History tab' },
    { key: 'B', description: 'Open branch picker' },
    { key: ',', description: 'Open settings' },
    { key: '?', description: 'Open this help' },
    { key: '`', description: 'Toggle terminal' },
    { key: 'Escape', description: 'Close overlay' },
  ]
</script>

{#if isOpen}
  <div class="overlay" role="presentation" onclick={onClose} onkeydown={handleOverlayKeyDown}>
    <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>Keyboard Shortcuts</h2>
        <button class="close-btn" onclick={onClose} aria-label="Close">✕</button>
      </div>

      <div class="modal-body">
        <table>
          <thead>
            <tr>
              <th>Shortcut</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {#each shortcuts as shortcut (shortcut.key)}
              <tr>
                <td class="key">{shortcut.key}</td>
                <td>{shortcut.description}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="modal-footer">
        <button class="btn-secondary" onclick={onClose}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--bg-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 6px;
    width: 90%;
    max-width: 560px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .close-btn {
    padding: 4px 8px;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    cursor: pointer;
    border-radius: 3px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--bg-secondary);
  }

  .modal-body {
    flex: 1;
    padding: 16px;
    overflow-y: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th {
    text-align: left;
    padding: 8px;
    font-weight: 600;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-secondary);
  }

  td {
    padding: 8px;
    border-bottom: 1px solid var(--border-inactive);
    color: var(--text-primary);
  }

  td.key {
    font-family: 'Monaco', 'Menlo', monospace;
    color: var(--status-blue);
    width: 35%;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary {
    padding: 6px 14px;
    font-size: 12px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    cursor: pointer;
  }
</style>

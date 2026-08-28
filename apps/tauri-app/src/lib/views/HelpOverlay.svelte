<script lang="ts">
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  const shortcuts = [
    { key: 'Ctrl/Cmd + Enter', description: 'Commit selected files' },
    { key: 'Ctrl/Cmd + G', description: 'Generate commit message with AI' },
    // Named after what the button says rather than after one of its states:
    // the chord runs whatever the sync ladder proposes.
    { key: 'Ctrl/Cmd + P', description: 'Run the sync action (fetch / pull / push / publish)' },
    { key: 'Ctrl/Cmd + R', description: 'Reload status, history and branches' },
    { key: 'Ctrl/Cmd + 1', description: 'Show Changes' },
    { key: 'Ctrl/Cmd + 2', description: 'Show History' },
    { key: 'Ctrl/Cmd + L', description: 'Toggle Changes / History tab' },
    { key: 'Ctrl/Cmd + B', description: 'Open branch menu' },
    { key: 'Ctrl/Cmd + ,', description: 'Open settings' },
    { key: 'Ctrl/Cmd + `', description: 'Toggle terminal' },
    { key: '?', description: 'Open this help' },
    // Only the topmost: a confirmation raised from a popover closes itself and
    // leaves the popover it came from standing.
    { key: 'Escape', description: 'Close the frontmost dialog or menu' },
  ]
</script>

{#if isOpen}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose()
    }}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1" use:dismissOnEscape={onClose}>
      <div class="modal-header">
        <h2>Keyboard Shortcuts</h2>
        <button class="close-btn" onclick={onClose} aria-label="Close">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
            <line x1="4" y1="4" x2="12" y2="12" />
            <line x1="12" y1="4" x2="4" y2="12" />
          </svg>
        </button>
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
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 480px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
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
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .modal-body {
    flex: 1;
    padding: 8px 16px 16px;
    overflow-y: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th {
    text-align: left;
    padding: 6px 8px;
    font-weight: 500;
    font-size: 11px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-inactive);
    background: transparent;
  }

  td {
    padding: 6px 8px;
    color: var(--text-primary);
  }

  td.key {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    width: 38%;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary {
    padding: 3px 14px;
    font-size: 12px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
  }

  .btn-secondary:hover {
    background: var(--surface-hover);
  }
</style>

<script lang="ts">
  import { configApi, type Config } from '$lib/api/commands'
  import { refreshConfig } from '$lib/stores/config'

  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  let config = $state<Config | null>(null)
  let isSaving = $state(false)
  let error = $state('')

  async function loadConfig() {
    try {
      config = await configApi.loadConfig()
      error = ''
    } catch (e) {
      error = String(e)
    }
  }

  async function handleSave() {
    if (!config) return
    isSaving = true
    error = ''
    try {
      await configApi.saveConfig(config)
      await refreshConfig()
      onClose()
    } catch (e) {
      error = String(e)
    } finally {
      isSaving = false
    }
  }

  function handleOverlayKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }

  $effect(() => {
    if (isOpen) loadConfig()
  })
</script>

{#if isOpen}
  <div class="overlay" role="presentation" onclick={onClose} onkeydown={handleOverlayKeyDown}>
    <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>Settings</h2>
        <button class="close-btn" onclick={onClose} aria-label="Close">✕</button>
      </div>

      <div class="modal-body">
        {#if error}
          <div class="error">{error}</div>
        {/if}

        {#if config}
          <h3>Appearance</h3>
          <div class="setting-group">
            <label for="theme-select">Theme</label>
            <select id="theme-select" bind:value={config.theme}>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
          </div>

          <h3>Diff</h3>
          <div class="setting-group">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={config.side_by_side_diff} />
              Side-by-side diff view
            </label>
          </div>
          <div class="setting-group">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={config.hide_whitespace} />
              Hide whitespace changes
            </label>
          </div>
          <div class="setting-group">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={config.syntax_highlighting} />
              Syntax highlighting
            </label>
          </div>
          <div class="setting-group">
            <label for="tab-size">Tab size</label>
            <input id="tab-size" type="number" bind:value={config.tab_size} min="1" max="16" />
          </div>

          <h3>Git</h3>
          <div class="setting-group">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={config.auto_fetch} />
              Auto fetch from remote
            </label>
          </div>
          <div class="setting-group">
            <label for="fetch-interval">Fetch interval (ms)</label>
            <input id="fetch-interval" type="number" bind:value={config.fetch_interval_ms} min="5000" step="1000" />
          </div>

          <h3>AI</h3>
          <div class="setting-group">
            <label for="provider-select">Provider</label>
            <select id="provider-select" bind:value={config.ai_provider}>
              <option value="claude">Claude</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>
          <div class="setting-group">
            <label for="ai-model">Model (optional)</label>
            <input id="ai-model" type="text" bind:value={config.ai_model} placeholder="sonnet / tavernari/git-commit-message:latest" />
          </div>
          <div class="setting-group">
            <label for="ollama-url">Ollama server URL</label>
            <input id="ollama-url" type="text" bind:value={config.ollama_server_url} />
          </div>
          <div class="setting-group">
            <label for="claude-timeout">Claude timeout (s)</label>
            <input id="claude-timeout" type="number" bind:value={config.claude_timeout_secs} min="10" />
          </div>

          <h3>Repository discovery</h3>
          <div class="setting-group">
            <label for="scan-depth">Scan depth</label>
            <input id="scan-depth" type="number" bind:value={config.scan_depth} min="1" max="10" />
          </div>
          <div class="setting-group">
            <label for="scan-paths">Scan paths (one per line)</label>
            <textarea
              id="scan-paths"
              class="paths-input"
              value={config.scan_paths.join('\n')}
              oninput={(e) => {
                if (config) {
                  config.scan_paths = (e.currentTarget.value || '').split('\n').map((s) => s.trim()).filter(Boolean)
                }
              }}
              rows="4"
            ></textarea>
          </div>
        {/if}
      </div>

      <div class="modal-footer">
        <button class="btn-secondary" onclick={onClose}>Cancel</button>
        <button class="btn-primary" onclick={handleSave} disabled={isSaving || !config}>
          {isSaving ? 'Saving…' : 'Save'}
        </button>
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
    max-width: 520px;
    max-height: 85vh;
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
    padding: 2px 6px;
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 4px;
    font-size: 12px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  .modal-body {
    flex: 1;
    padding: 12px 16px 16px;
    overflow-y: auto;
  }

  .modal-body h3 {
    margin: 0 0 8px 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .modal-body h3:not(:first-child) {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--border-inactive);
  }

  .error {
    padding: 8px 12px;
    margin-bottom: 12px;
    color: var(--status-red);
    font-size: 12px;
  }

  .setting-group {
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .setting-group label:not(.checkbox-label) {
    flex: 0 0 140px;
    font-size: 12px;
    color: var(--text-secondary);
    text-align: right;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    color: var(--text-primary) !important;
    font-size: 13px !important;
    margin-left: 152px;
  }

  .checkbox-label input[type='checkbox'] {
    width: 14px;
    height: 14px;
    cursor: pointer;
    accent-color: var(--border-active);
  }

  .setting-group select,
  .setting-group input[type='number'],
  .setting-group input[type='text'] {
    flex: 1;
    padding: 4px 8px;
    font-size: 13px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .paths-input {
    flex: 1;
    padding: 6px 8px;
    font-size: 12px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: var(--font-mono);
    resize: vertical;
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
    border: 1px solid var(--border-strong);
    cursor: pointer;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
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

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

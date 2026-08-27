<script lang="ts">
  import { configApi, terminalApi, type Config, type ShellOption } from '$lib/api/commands'
  import { refreshConfig } from '$lib/stores/config'

  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  let config = $state<Config | null>(null)
  let isSaving = $state(false)
  let error = $state('')
  /** Shells probed on this machine, best-first. Empty until loaded. */
  let shells = $state<ShellOption[]>([])
  /** Bound to the picker; `''` is the "Automatic" sentinel the <select> needs
   *  in place of `Config.terminal_shell`'s absent value. */
  let shellChoice = $state('')

  /** What "Automatic" resolves to, so the choice isn't a mystery. */
  let autoShellLabel = $derived(shells[0]?.label ?? '')

  /**
   * Bounds for the numeric fields, matching the native client's
   * `SettingsStore` ranges. The `min`/`max` attributes on `<input
   * type=number>` are advisory only — typing 999 or clearing the field both
   * pass straight through, and Svelte's numeric binding turns an emptied
   * field into `null`, which used to reach the backend and fail with a raw
   * serde error the user couldn't escape without refilling the field. Every
   * value is clamped on the way out, and the clamped value is written back
   * into the form so the correction is visible rather than silent.
   */
  const BOUNDS = {
    tabSize: { min: 1, max: 16, fallback: 4 },
    // Milliseconds on the wire; 5 s–1 h, the native client's range in seconds.
    fetchIntervalMs: { min: 5_000, max: 3_600_000, fallback: 30_000 },
    claudeTimeoutSecs: { min: 10, max: 3_600, fallback: 120 },
    scanDepth: { min: 1, max: 10, fallback: 3 },
  } as const

  /** The nearest in-range integer, or the default when the field is empty or
   *  unparseable. */
  function clamp(value: number | null | undefined, b: { min: number; max: number; fallback: number }): number {
    if (value === null || value === undefined || !Number.isFinite(value)) return b.fallback
    return Math.min(Math.max(Math.round(value), b.min), b.max)
  }

  async function loadConfig() {
    try {
      const [cfg, available] = await Promise.all([
        configApi.loadConfig(),
        // Non-fatal: an empty list just leaves the picker with "Automatic".
        terminalApi.listShells().catch((e) => {
          console.warn('[settings] shell discovery failed', e)
          return [] as ShellOption[]
        }),
      ])
      config = cfg
      shells = available
      // A preference whose shell is no longer installed shows as Automatic,
      // matching what the backend would actually launch.
      shellChoice = available.some((s) => s.id === cfg.terminal_shell)
        ? (cfg.terminal_shell ?? '')
        : ''
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
      config.terminal_shell = shellChoice || undefined
      config.tab_size = clamp(config.tab_size, BOUNDS.tabSize)
      config.fetch_interval_ms = clamp(config.fetch_interval_ms, BOUNDS.fetchIntervalMs)
      config.claude_timeout_secs = clamp(config.claude_timeout_secs, BOUNDS.claudeTimeoutSecs)
      config.scan_depth = clamp(config.scan_depth, BOUNDS.scanDepth)
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
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose()
    }}
    onkeydown={handleOverlayKeyDown}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <div class="modal-header">
        <h2>Settings</h2>
        <button class="close-btn" onclick={onClose} aria-label="Close">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
            <line x1="4" y1="4" x2="12" y2="12" />
            <line x1="12" y1="4" x2="4" y2="12" />
          </svg>
        </button>
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
            <input
              id="tab-size"
              type="number"
              bind:value={config.tab_size}
              min={BOUNDS.tabSize.min}
              max={BOUNDS.tabSize.max}
            />
          </div>

          <h3>Terminal</h3>
          <div class="setting-group">
            <label for="terminal-shell">Shell</label>
            <select id="terminal-shell" bind:value={shellChoice}>
              <option value="">Automatic{autoShellLabel ? ` (${autoShellLabel})` : ''}</option>
              {#each shells as shell (shell.id)}
                <option value={shell.id}>{shell.label}</option>
              {/each}
            </select>
            <p class="setting-hint">
              Only shells found on this machine are listed. Applies to new terminal sessions.
            </p>
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
            <input
              id="fetch-interval"
              type="number"
              bind:value={config.fetch_interval_ms}
              min={BOUNDS.fetchIntervalMs.min}
              max={BOUNDS.fetchIntervalMs.max}
              step="1000"
            />
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
            <input
              id="claude-timeout"
              type="number"
              bind:value={config.claude_timeout_secs}
              min={BOUNDS.claudeTimeoutSecs.min}
              max={BOUNDS.claudeTimeoutSecs.max}
            />
          </div>

          <h3>Repository discovery</h3>
          <div class="setting-group">
            <label for="scan-depth">Scan depth</label>
            <input
              id="scan-depth"
              type="number"
              bind:value={config.scan_depth}
              min={BOUNDS.scanDepth.min}
              max={BOUNDS.scanDepth.max}
            />
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
              rows="6"
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
    /* Lets a full-width hint drop onto its own line under its control
       instead of competing with it for horizontal space. */
    flex-wrap: wrap;
  }

  .setting-hint {
    flex-basis: 100%;
    margin: 2px 0 0;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-muted);
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
    line-height: 1.5;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: var(--font-mono);
    /* Only vertical, and never below four rows plus padding: the handle used
       to collapse the box to a sliver that hid every configured path. The
       `rows` attribute sets the taller starting height. */
    resize: vertical;
    min-height: 88px;
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

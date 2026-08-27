<script lang="ts">
  import {
    configApi,
    terminalApi,
    type Config,
    type ConfigBounds,
    type ShellOption,
  } from '$lib/api/commands'
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
   * Bounds for the numeric fields, read from core — the same declaration the
   * writer clamps against, rather than a third copy of these numbers in a
   * third unit. The `min`/`max` attributes on `<input type=number>` are
   * advisory only: typing 999 or clearing the field both pass straight
   * through, and Svelte's numeric binding turns an emptied field into `null`.
   * Core clamps and normalizes on write and hands back the corrected config,
   * which is written straight back into the form so the correction is visible
   * rather than silent.
   */
  let bounds = $state<ConfigBounds | null>(null)

  async function loadConfig() {
    try {
      const [cfg, limits, available] = await Promise.all([
        configApi.loadConfig(),
        configApi.configBounds(),
        // Non-fatal: an empty list just leaves the picker with "Automatic".
        terminalApi.listShells().catch((e) => {
          console.warn('[settings] shell discovery failed', e)
          return [] as ShellOption[]
        }),
      ])
      config = cfg
      bounds = limits
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
      // A patch naming exactly the fields this form owns. The whole-object
      // write it replaces posted the config as it looked when the dialog
      // *opened*, reverting whatever the other client had saved since — and
      // an emptied numeric field reached the backend as `null` and failed
      // with a raw serde error. Core clamps, normalizes and returns the
      // result, which goes straight back into the form.
      config = await configApi.patchConfig({
        terminal_shell: shellChoice,
        tab_size: config.tab_size ?? undefined,
        fetch_interval_ms: config.fetch_interval_ms ?? undefined,
        scan_depth: config.scan_depth ?? undefined,
        theme: config.theme,
        ai_provider: config.ai_provider,
        auto_fetch: config.auto_fetch,
        syntax_highlighting: config.syntax_highlighting,
        scan_paths: config.scan_paths,
        side_by_side_diff: config.side_by_side_diff,
        hide_whitespace: config.hide_whitespace,
        claude_model: config.claude.model ?? '',
        claude_timeout_secs: config.claude.timeout_secs ?? undefined,
        ollama_model: config.ollama.model ?? '',
        ollama_server_url: config.ollama.server_url,
        ollama_timeout_secs: config.ollama.timeout_secs ?? undefined,
      })
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
              min={bounds?.tab_size.min}
              max={bounds?.tab_size.max}
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
              min={bounds?.fetch_interval_ms.min}
              max={bounds?.fetch_interval_ms.max}
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
          <!-- One model field per provider: a single shared one meant a model
               set for Claude was handed to Ollama, which has never heard of
               it, so Generate failed with nothing on screen explaining why. -->
          {#if config.ai_provider === 'ollama'}
            <div class="setting-group">
              <label for="ollama-model">Model (optional)</label>
              <input
                id="ollama-model"
                type="text"
                bind:value={config.ollama.model}
                placeholder="tavernari/git-commit-message:latest"
              />
            </div>
            <div class="setting-group">
              <label for="ollama-url">Ollama server URL</label>
              <input id="ollama-url" type="text" bind:value={config.ollama.server_url} />
            </div>
            <div class="setting-group">
              <label for="ollama-timeout">Ollama timeout (s)</label>
              <input
                id="ollama-timeout"
                type="number"
                bind:value={config.ollama.timeout_secs}
                min={bounds?.ai_timeout_secs.min}
                max={bounds?.ai_timeout_secs.max}
              />
            </div>
          {:else}
            <div class="setting-group">
              <label for="claude-model">Model (optional)</label>
              <input
                id="claude-model"
                type="text"
                bind:value={config.claude.model}
                placeholder="sonnet"
              />
            </div>
            <div class="setting-group">
              <label for="claude-timeout">Claude timeout (s)</label>
              <input
                id="claude-timeout"
                type="number"
                bind:value={config.claude.timeout_secs}
                min={bounds?.ai_timeout_secs.min}
                max={bounds?.ai_timeout_secs.max}
              />
            </div>
          {/if}

          <h3>Repository discovery</h3>
          <div class="setting-group">
            <label for="scan-depth">Scan depth</label>
            <input
              id="scan-depth"
              type="number"
              bind:value={config.scan_depth}
              min={bounds?.scan_depth.min}
              max={bounds?.scan_depth.max}
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

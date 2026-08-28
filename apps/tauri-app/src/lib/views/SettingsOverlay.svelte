<script lang="ts">
  /**
   * Settings — instant-apply, like the native window and like macOS settings
   * generally.
   *
   * Every control patches its own field as it changes: discrete controls
   * (checkboxes, pickers) on the click, text and numeric fields when they lose
   * focus or take a Return, which is what an `<input>`'s `change` event already
   * means. There is no Save button, so there is nothing to forget to press and
   * nothing to cancel — the config on disk is what the form shows.
   *
   * Two things follow from that and are load-bearing:
   * - **A patch names only the field that changed.** The whole-object write
   *   this replaced posted the config as it looked when the dialog *opened*,
   *   silently reverting whatever the other client had saved meanwhile. Core
   *   clamps and normalizes, and hands the result back, so an out-of-range
   *   entry corrects itself in front of the user instead of being dropped.
   * - **A write that fails puts its control back.** With no Save button
   *   pending, a control still showing the rejected value would be claiming a
   *   setting that isn't on disk.
   *
   * Scan paths are the one field outside instant-apply, behind an Edit ▸ Done
   * cycle: they decide which repositories exist as far as the app is concerned,
   * and half a typed line is a different setting rather than a smaller one.
   */
  import {
    configApi,
    terminalApi,
    type ConfigBounds,
    type ConfigPatch,
    type ShellOption,
  } from '$lib/api/commands'
  import { applyConfig, config as configStore, refreshConfig } from '$lib/stores/config'
  import { rediscoverRepos } from '$lib/services/repoDiscovery'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  /**
   * The one config in the client. Reading the shared store rather than keeping
   * a second copy is what stops the form and the rest of the app from
   * disagreeing — and it means a change made in the native client, which
   * arrives on the next window activation, shows up here rather than being
   * overwritten by a stale form.
   */
  const config = $derived($configStore)

  let error = $state('')
  /** Shells probed on this machine, best-first. Empty until loaded. */
  let shells = $state<ShellOption[]>([])

  /**
   * Bounds for the numeric fields, read from core — the same declaration the
   * writer clamps against, rather than a third copy of these numbers in a
   * third unit. The `min`/`max` attributes on `<input type=number>` are
   * advisory only: typing 999 or clearing the field both pass straight
   * through. Core clamps on write and hands back the corrected config, which
   * the form re-renders from, so the correction is visible rather than silent.
   */
  let bounds = $state<ConfigBounds | null>(null)

  /** What "Automatic" resolves to, so the choice isn't a mystery. */
  const autoShellLabel = $derived(shells[0]?.label ?? '')

  /** A preference whose shell is no longer installed shows as Automatic,
   *  matching what the backend would actually launch. */
  const shellChoice = $derived(
    shells.some((s) => s.id === config?.terminal_shell) ? (config?.terminal_shell ?? '') : '',
  )

  /**
   * Seconds on screen, milliseconds on the wire — the native window's split,
   * so neither client makes the user count zeroes in `1800000`. Floored at one
   * second so a hand-edited sub-second value still renders as a number.
   */
  const fetchIntervalSeconds = $derived(Math.max(Math.floor((config?.fetch_interval_ms ?? 0) / 1000), 1))
  const intervalBounds = $derived(
    bounds
      ? {
          min: Math.floor(bounds.fetch_interval_ms.min / 1000),
          max: Math.floor(bounds.fetch_interval_ms.max / 1000),
        }
      : null,
  )

  /** Scan paths, mid-edit. Nothing is written until Done, so leaving the
   *  dialog — by any route — discards the draft. */
  let pathsEditing = $state(false)
  let pathsDraft = $state('')

  async function loadSettings() {
    pathsEditing = false
    error = ''
    try {
      const [cfg, limits, available] = await Promise.all([
        // Re-read on open rather than trusting the store: the other client may
        // have written the file since this window last looked at it.
        refreshConfig(),
        configApi.configBounds(),
        // Non-fatal: an empty list just leaves the picker with "Automatic".
        terminalApi.listShells().catch((e) => {
          console.warn('[settings] shell discovery failed', e)
          return [] as ShellOption[]
        }),
      ])
      bounds = limits
      shells = available
      if (!cfg) error = 'Could not read the configuration file.'
    } catch (e) {
      error = String(e)
    }
  }

  /**
   * Writes in flight, chained. Two quick edits must land in the order they were
   * made — `patch_config` is a read-modify-write, so overlapping calls would
   * race on the file each of them just read.
   */
  let writes: Promise<void> = Promise.resolve()

  /**
   * Bumped to rebuild every control from the config on disk.
   *
   * The controls render from `config`, so they repaint whenever it changes —
   * which covers most of what core's clamp does. The two cases it doesn't are
   * both cases where the *DOM* holds a value the config never took: a rejected
   * write, and an entry core clamped back to the value it already had (999
   * typed into a field already at its maximum). Neither changes `config`, so
   * nothing would repaint without this.
   */
  let formSeq = $state(0)

  function patch(fields: ConfigPatch): void {
    writes = writes.then(async () => {
      const before = $configStore
      try {
        const updated = await configApi.patchConfig(fields)
        await applyConfig(updated)
        error = ''
        if (before && JSON.stringify(before) === JSON.stringify(updated)) formSeq += 1
        // The scan paths are what discovery walks and the depth is how far, so
        // a change to either re-walks now: the setting takes effect where it
        // was made, rather than on a later dialog dismissal.
        if (fields.scan_paths !== undefined || fields.scan_depth !== undefined) {
          void rediscoverRepos()
        }
      } catch (e) {
        // Nothing landed, so the control must stop claiming it did.
        error = String(e)
        formSeq += 1
      }
    })
  }

  /**
   * Commit a numeric field, or put it back.
   *
   * An emptied or unparseable `<input type=number>` reads as `NaN`, and a patch
   * naming it would reach the backend as `null` and fail with a raw serde
   * error. Snapping the control back to what is on disk says "that was not a
   * number" without a sentence about it.
   */
  function commitNumber(
    e: Event & { currentTarget: HTMLInputElement },
    fallback: number,
    apply: (value: number) => void,
  ): void {
    const value = e.currentTarget.valueAsNumber
    if (Number.isNaN(value)) {
      e.currentTarget.value = String(fallback)
      return
    }
    apply(Math.round(value))
  }

  function togglePathsEdit(): void {
    if (!config) return
    if (!pathsEditing) {
      pathsDraft = config.scan_paths.join('\n')
      pathsEditing = true
      return
    }
    pathsEditing = false
    patch({ scan_paths: pathsDraft.split('\n').map((s) => s.trim()).filter(Boolean) })
  }

  $effect(() => {
    if (isOpen) loadSettings()
  })
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

        {#key formSeq}
        {#if config}
          <h3>Appearance</h3>
          <div class="setting-group">
            <label for="theme-select">Theme</label>
            <select
              id="theme-select"
              value={config.theme}
              onchange={(e) => patch({ theme: e.currentTarget.value })}
            >
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
          </div>

          <h3>Git</h3>
          <div class="setting-group">
            <label class="checkbox-label">
              <input
                type="checkbox"
                checked={config.auto_fetch}
                onchange={(e) => patch({ auto_fetch: e.currentTarget.checked })}
              />
              Automatically fetch from remotes
            </label>
          </div>
          <div class="setting-group">
            <label for="fetch-interval">Fetch interval (s)</label>
            <input
              id="fetch-interval"
              type="number"
              value={fetchIntervalSeconds}
              min={intervalBounds?.min}
              max={intervalBounds?.max}
              step="5"
              disabled={!config.auto_fetch}
              onchange={(e) =>
                commitNumber(e, fetchIntervalSeconds, (n) => patch({ fetch_interval_ms: n * 1000 }))}
            />
          </div>
          <p class="section-footer">
            Applies to the open repository within one interval — no restart needed.
          </p>

          <h3>Diff</h3>
          <div class="setting-group">
            <label class="checkbox-label">
              <input
                type="checkbox"
                checked={config.side_by_side_diff}
                onchange={(e) => patch({ side_by_side_diff: e.currentTarget.checked })}
              />
              Side-by-side diff view
            </label>
          </div>
          <div class="setting-group">
            <label class="checkbox-label">
              <input
                type="checkbox"
                checked={config.hide_whitespace}
                onchange={(e) => patch({ hide_whitespace: e.currentTarget.checked })}
              />
              Hide whitespace changes
            </label>
          </div>
          <div class="setting-group">
            <label class="checkbox-label">
              <input
                type="checkbox"
                checked={config.syntax_highlighting}
                onchange={(e) => patch({ syntax_highlighting: e.currentTarget.checked })}
              />
              Syntax highlighting
            </label>
          </div>
          <div class="setting-group">
            <label for="tab-size">Tab size</label>
            <input
              id="tab-size"
              type="number"
              value={config.tab_size}
              min={bounds?.tab_size.min}
              max={bounds?.tab_size.max}
              onchange={(e) =>
                commitNumber(e, config?.tab_size ?? 4, (n) => patch({ tab_size: n }))}
            />
          </div>
          <p class="section-footer">Applies to the open diff immediately.</p>

          <h3>Repository Discovery</h3>
          <div class="setting-group">
            <label for="scan-paths">Folders to scan</label>
            <!-- Read-only until Edit, the macOS list-editor pattern: this is
                 the one setting that decides which repositories the app can
                 see at all, and a half-typed line is a different folder rather
                 than a shorter one. Nothing is written until Done. -->
            <textarea
              id="scan-paths"
              class="paths-input"
              readonly={!pathsEditing}
              value={pathsEditing ? pathsDraft : config.scan_paths.join('\n')}
              oninput={(e) => (pathsDraft = e.currentTarget.value)}
              rows="6"
            ></textarea>
            <div class="paths-actions">
              <button class="btn-secondary" onclick={togglePathsEdit}>
                {pathsEditing ? 'Done' : 'Edit'}
              </button>
            </div>
          </div>
          <div class="setting-group">
            <label for="scan-depth">Scan depth</label>
            <input
              id="scan-depth"
              type="number"
              value={config.scan_depth}
              min={bounds?.scan_depth.min}
              max={bounds?.scan_depth.max}
              onchange={(e) =>
                commitNumber(e, config?.scan_depth ?? 3, (n) => patch({ scan_depth: n }))}
            />
          </div>
          <p class="section-footer">
            One folder per line (~ allowed). The repository switcher searches these for git
            repositories.
          </p>

          <h3>Terminal</h3>
          <div class="setting-group">
            <label for="terminal-shell">Shell</label>
            <select
              id="terminal-shell"
              value={shellChoice}
              onchange={(e) => patch({ terminal_shell: e.currentTarget.value })}
            >
              <option value="">Automatic{autoShellLabel ? ` (${autoShellLabel})` : ''}</option>
              {#each shells as shell (shell.id)}
                <option value={shell.id}>{shell.label}</option>
              {/each}
            </select>
          </div>
          <p class="section-footer">
            Only shells found on this machine are listed. Applies to new terminal sessions.
          </p>

          <h3>AI Commit Messages</h3>
          <div class="setting-group">
            <label for="provider-select">Provider</label>
            <select
              id="provider-select"
              value={config.ai_provider}
              onchange={(e) => patch({ ai_provider: e.currentTarget.value })}
            >
              <option value="claude">Claude</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>
          <!-- One model field per provider: a single shared one meant a model
               set for Claude was handed to Ollama, which has never heard of
               it, so Generate failed with nothing on screen explaining why. -->
          {#if config.ai_provider === 'ollama'}
            <div class="setting-group">
              <label for="ollama-model">Model</label>
              <input
                id="ollama-model"
                type="text"
                value={config.ollama.model ?? ''}
                placeholder="tavernari/git-commit-message:latest"
                onchange={(e) => patch({ ollama_model: e.currentTarget.value })}
              />
            </div>
            <div class="setting-group">
              <label for="ollama-url">Ollama server URL</label>
              <input
                id="ollama-url"
                type="text"
                value={config.ollama.server_url}
                placeholder="http://localhost:11434"
                onchange={(e) => patch({ ollama_server_url: e.currentTarget.value })}
              />
            </div>
            <div class="setting-group">
              <label for="ollama-timeout">Ollama timeout (s)</label>
              <input
                id="ollama-timeout"
                type="number"
                value={config.ollama.timeout_secs}
                min={bounds?.ai_timeout_secs.min}
                max={bounds?.ai_timeout_secs.max}
                onchange={(e) =>
                  commitNumber(e, config?.ollama.timeout_secs ?? 120, (n) =>
                    patch({ ollama_timeout_secs: n }),
                  )}
              />
            </div>
          {:else}
            <div class="setting-group">
              <label for="claude-model">Model</label>
              <input
                id="claude-model"
                type="text"
                value={config.claude.model ?? ''}
                placeholder="sonnet"
                onchange={(e) => patch({ claude_model: e.currentTarget.value })}
              />
            </div>
            <div class="setting-group">
              <label for="claude-timeout">Claude timeout (s)</label>
              <input
                id="claude-timeout"
                type="number"
                value={config.claude.timeout_secs}
                min={bounds?.ai_timeout_secs.min}
                max={bounds?.ai_timeout_secs.max}
                onchange={(e) =>
                  commitNumber(e, config?.claude.timeout_secs ?? 120, (n) =>
                    patch({ claude_timeout_secs: n }),
                  )}
              />
            </div>
          {/if}
          <p class="section-footer">
            Used by Generate in the commit composer. Each provider keeps its own model; leave it
            empty for that provider's default.
          </p>
        {/if}
        {/key}
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
    /* Lets a full-width action row drop onto its own line under its control
       instead of competing with it for horizontal space. */
    flex-wrap: wrap;
  }

  /* What a whole section applies to, and when. Sits under the last control of
     the section it describes — the scope of "immediately" is the section, not
     any one checkbox in it. */
  .section-footer {
    margin: 6px 0 0 152px;
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

  .setting-group input:disabled {
    opacity: 0.5;
  }

  /* Numeric settings are plain fields — the spinner arrows are a second way to
     change a value that already has a keyboard, and each click of one is
     another write. */
  .setting-group input[type='number'] {
    appearance: textfield;
    font-variant-numeric: tabular-nums;
  }

  .setting-group input[type='number']::-webkit-outer-spin-button,
  .setting-group input[type='number']::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
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

  .paths-input:read-only {
    color: var(--text-secondary);
    background: var(--bg-secondary);
  }

  .paths-actions {
    flex-basis: 100%;
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
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
    font-weight: 500;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    cursor: pointer;
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background: var(--surface-hover);
  }
</style>

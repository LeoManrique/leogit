<script lang="ts">
  import { get } from 'svelte/store'
  import { repoState, canCommit } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { gitApi, aiApi, configApi, type AiProviderConfig } from '$lib/api/commands'

  let summary = $state('')
  let description = $state('')
  let provider = $state<'claude' | 'ollama'>('claude')
  let isGenerating = $state(false)
  let isCommitting = $state(false)
  let error = $state<string | null>(null)
  let charCount = $derived(summary.length)
  let canSubmit = $derived($canCommit && summary.trim().length > 0)

  async function handleGenerate() {
    const state = get(repoState)
    const repoPath = $appState.repoPath
    if (!repoPath) return

    if (state.selectedFiles.size === 0) {
      error = 'No files selected'
      return
    }

    isGenerating = true
    error = null

    try {
      const files = Array.from(state.selectedFiles)
        .map((path) => state.status.files.find((f) => f.path === path))
        .filter((f): f is NonNullable<typeof f> => Boolean(f))

      const diffStr = await gitApi.getSelectedDiff(repoPath, files)

      let cfg: AiProviderConfig = { provider }
      try {
        const fullConfig = await configApi.loadConfig()
        cfg = {
          provider,
          model: fullConfig.ai_model,
          api_key: fullConfig.ai_api_key,
          base_url: fullConfig.ollama_server_url,
        }
      } catch {}

      const message = await aiApi.generateCommitMessage(diffStr, provider, cfg)
      summary = message.title
      description = message.description
    } catch (err) {
      error = `Generate failed: ${String(err)}`
    } finally {
      isGenerating = false
    }
  }

  function cycleProvider() {
    provider = provider === 'claude' ? 'ollama' : 'claude'
  }

  async function handleCommit() {
    if (!summary.trim()) {
      error = 'Summary is required'
      return
    }
    const repoPath = $appState.repoPath
    if (!repoPath) return

    isCommitting = true
    error = null

    try {
      const fullMessage = await gitApi.formatCommitMessage(summary, description)
      await gitApi.commit(repoPath, fullMessage)
      summary = ''
      description = ''
      repoState.update((s) => ({ ...s, selectedFiles: new Set(), userDeselected: new Set() }))
    } catch (err) {
      error = `Commit failed: ${String(err)}`
    } finally {
      isCommitting = false
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    const meta = e.ctrlKey || e.metaKey
    if (meta && e.key === 'g') {
      e.preventDefault()
      if (!isGenerating) handleGenerate()
    } else if (meta && e.key === 'p') {
      e.preventDefault()
      cycleProvider()
    } else if (meta && e.key === 'Enter') {
      e.preventDefault()
      if (canSubmit && !isCommitting) handleCommit()
    }
  }
</script>

<div class="commit-message-container" onkeydown={handleKeyDown} role="form">
  <div class="summary-section">
    <input
      id="summary-input"
      type="text"
      class="summary-input"
      placeholder="Summary"
      bind:value={summary}
      maxlength="200"
      disabled={isGenerating || isCommitting}
    />
    <span class="char-count" class:warning={charCount > 72}>{charCount}/72</span>
  </div>

  <div class="description-section">
    <textarea
      id="description-input"
      class="description-input"
      placeholder="Description"
      bind:value={description}
      disabled={isGenerating || isCommitting}
    ></textarea>
  </div>

  {#if error}
    <div class="error-message">{error}</div>
  {/if}

  <div class="button-bar">
    <div class="button-group">
      <select class="provider-select" bind:value={provider} disabled={isGenerating || isCommitting}>
        <option value="claude">Claude</option>
        <option value="ollama">Ollama</option>
      </select>
      <button
        class="action-button"
        onclick={handleGenerate}
        disabled={isGenerating || isCommitting || $repoState.selectedFiles.size === 0}
        title="Generate (Ctrl+G)"
      >
        {isGenerating ? 'Generating…' : 'Generate'}
      </button>
    </div>

    <button
      class="commit-button"
      onclick={handleCommit}
      disabled={!canSubmit || isCommitting}
      title="Commit (Ctrl+Enter)"
    >
      {isCommitting ? 'Committing…' : 'Commit'}
    </button>
  </div>
</div>

<style>
  .commit-message-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-inactive);
  }

  .summary-section {
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .description-section {
    display: flex;
    flex-direction: column;
  }

  .char-count {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 10px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .char-count.warning {
    color: var(--status-yellow);
  }

  .summary-input {
    height: 28px;
    font-size: 13px;
    padding: 4px 48px 4px 8px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .summary-input:focus,
  .description-input:focus,
  .provider-select:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  .summary-input:disabled,
  .description-input:disabled,
  .provider-select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .description-input {
    height: 96px;
    font-size: 13px;
    padding: 8px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
    resize: none;
    overflow-y: auto;
  }

  .error-message {
    padding: 6px 8px;
    color: var(--status-red);
    font-size: 11px;
  }

  .button-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    justify-content: space-between;
  }

  .button-group {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .provider-select {
    font-size: 12px;
    padding: 3px 8px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
    cursor: pointer;
  }

  .action-button,
  .commit-button {
    padding: 3px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
    border: 1px solid var(--border-strong);
  }

  .action-button {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .action-button:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .action-button:disabled,
  .commit-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .commit-button {
    background: var(--border-active);
    color: #ffffff;
    border-color: var(--border-active);
    padding: 3px 16px;
    font-weight: 500;
  }

  .commit-button:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }
</style>

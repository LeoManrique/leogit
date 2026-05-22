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
    <div class="summary-header">
      <label for="summary-input">Summary</label>
      <span class="char-count" class:warning={charCount > 72}>{charCount}/72</span>
    </div>
    <input
      id="summary-input"
      type="text"
      class="summary-input"
      placeholder="Commit summary"
      bind:value={summary}
      maxlength="200"
      disabled={isGenerating || isCommitting}
    />
  </div>

  <div class="description-section">
    <label for="description-input">Description</label>
    <textarea
      id="description-input"
      class="description-input"
      placeholder="Detailed description (optional)"
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

  label {
    display: block;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-secondary);
    margin-bottom: 4px;
  }

  .summary-section,
  .description-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .summary-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .char-count {
    font-size: 11px;
    color: var(--text-muted);
  }

  .char-count.warning {
    color: var(--status-yellow);
    font-weight: 500;
  }

  .summary-input {
    height: 32px;
    font-size: 13px;
    padding: 6px 8px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    font-family: inherit;
  }

  .summary-input:focus,
  .description-input:focus,
  .provider-select:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 3px var(--cursor-bg);
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
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', 'Courier New', monospace;
    resize: none;
    overflow-y: auto;
  }

  .error-message {
    padding: 8px;
    background: rgba(248, 81, 73, 0.1);
    color: var(--status-red);
    border: 1px solid rgba(248, 81, 73, 0.3);
    border-radius: 4px;
    font-size: 11px;
  }

  .button-bar {
    display: flex;
    gap: 8px;
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
    padding: 6px 8px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    font-family: inherit;
    cursor: pointer;
  }

  .action-button,
  .commit-button {
    padding: 6px 12px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 150ms ease;
    border: 1px solid var(--border-inactive);
  }

  .action-button {
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .action-button:hover:not(:disabled) {
    border-color: var(--border-active);
  }

  .action-button:disabled,
  .commit-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .commit-button {
    background: var(--status-green);
    color: white;
    border-color: var(--status-green);
    padding: 6px 16px;
    font-weight: 600;
  }

  .commit-button:hover:not(:disabled) {
    opacity: 0.9;
  }
</style>

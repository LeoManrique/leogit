<script lang="ts">
  import { get } from 'svelte/store'
  import { repoState, canCommit } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { gitApi, aiApi, configApi, type AiProviderConfig } from '$lib/api/commands'

  interface Props {
    onCommitted?: () => void
    onStopAmending?: () => void
  }

  let { onCommitted, onStopAmending }: Props = $props()

  let summary = $state('')
  let description = $state('')
  let provider = $state<'claude' | 'ollama'>('claude')
  let isGenerating = $state(false)
  let isCommitting = $state(false)
  let error = $state<string | null>(null)
  let charCount = $derived(summary.length)

  // Co-author trailers preserved from the commit being amended, re-applied via
  // format_commit_message on commit. Plain trailers, e.g. "Name <email>".
  let amendCoAuthors = $state<string[]>([])

  // True while the user is editing the most recent commit instead of creating
  // a new one. Driven by repoState.commitToAmend (set from the History context
  // menu via MainLayout).
  const isAmending = $derived($repoState.commitToAmend !== null)

  // When exactly one file is staged, default the summary to a GitHub-Desktop
  // style "Create/Delete/Update <file>" so the most common commit (one file)
  // needs zero typing. Empty when 0 or many files are staged, so multi-file
  // commits still require a real summary.
  const autoSummary = $derived.by(() => {
    const sel = $repoState.selectedFiles
    if (sel.size !== 1) return ''
    const [path] = sel
    const file = $repoState.status.files.find((f) => f.path === path)
    if (!file) return ''
    const verb =
      file.status === 'New' ? 'Create' : file.status === 'Deleted' ? 'Delete' : 'Update'
    return `${verb} ${file.display_name}`
  })

  // What actually gets committed: the typed summary if any, otherwise the
  // single-file auto-summary. Also drives the input placeholder so the user
  // sees the message they'll commit before typing.
  const effectiveSummary = $derived(summary.trim() || autoSummary)
  const summaryPlaceholder = $derived(autoSummary || 'Summary')

  // Relaxed submit gate when amending: git allows --amend with no staged files
  // (message-only edit). Outside amend mode, canCommit requires file selection.
  // Uses effectiveSummary so a one-file commit can submit with a blank input.
  const canSubmit = $derived(effectiveSummary.length > 0 && (isAmending || $canCommit))

  // Parse Co-Authored-By trailers out of `commit.trailers`. The trailers list
  // comes from `%(trailers:unfold,only)` in git's log format, one trailer per
  // line. We only preserve co-author trailers; anything else (Signed-off-by,
  // Reviewed-by, etc.) is left for the user to re-add manually if they care.
  function extractCoAuthors(trailers: string[]): string[] {
    const out: string[] = []
    for (const raw of trailers) {
      const m = raw.match(/^\s*Co-Authored-By:\s*(.+?)\s*$/i)
      if (m && m[1]) out.push(m[1])
    }
    return out
  }

  // Strip Co-Authored-By lines from a commit body when pre-filling the
  // description, since the trailers will be re-applied via format_commit_message.
  function stripCoAuthorLines(body: string): string {
    return body
      .split('\n')
      .filter((line) => !/^\s*Co-Authored-By:/i.test(line))
      .join('\n')
      .trimEnd()
  }

  // When entering amend mode, pre-fill the composer from the target commit.
  // When leaving (commit-to-amend → null), clear the composer back to empty so
  // the user doesn't accidentally re-submit the amended message as a new commit.
  let lastAmendSha = $state<string | null>(null)
  $effect(() => {
    const target = $repoState.commitToAmend
    if (target !== null && target.sha !== lastAmendSha) {
      lastAmendSha = target.sha
      amendCoAuthors = extractCoAuthors(target.trailers)
      summary = target.summary
      description = stripCoAuthorLines(target.body)
    } else if (target === null && lastAmendSha !== null) {
      lastAmendSha = null
      amendCoAuthors = []
      summary = ''
      description = ''
    }
  })

  // One-shot prefill seed used by Undo Commit. MainLayout sets it on the store
  // after `git reset --mixed`; we copy values into the composer and clear the
  // seed so the effect doesn't re-fire on subsequent renders.
  $effect(() => {
    const seed = $repoState.restoreMessage
    if (seed !== null) {
      // Clear immediately — Svelte 5 re-runs the effect with seed === null,
      // and the early return prevents an infinite loop.
      repoState.update((s) => ({ ...s, restoreMessage: null }))
      summary = seed.summary
      description = seed.description
      amendCoAuthors = seed.coAuthors
    }
  })

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
    // effectiveSummary falls back to the single-file auto-summary, so a blank
    // input is fine as long as something resolves; only a truly empty message
    // (no input, no auto-summary) is rejected.
    if (!effectiveSummary) {
      error = 'Summary is required'
      return
    }
    const repoPath = $appState.repoPath
    if (!repoPath) return

    const state = get(repoState)
    const files = Array.from(state.selectedFiles)
      .map((path) => state.status.files.find((f) => f.path === path))
      .filter((f): f is NonNullable<typeof f> => Boolean(f))
    // Amend allows a message-only commit, so an empty file list is only an
    // error in the non-amend path.
    if (files.length === 0 && !isAmending) {
      error = 'No files selected'
      return
    }

    isCommitting = true
    error = null

    try {
      const fullMessage = await gitApi.formatCommitMessage(effectiveSummary, description, amendCoAuthors)
      await gitApi.commit(repoPath, fullMessage, files, isAmending)
      summary = ''
      description = ''
      amendCoAuthors = []
      repoState.update((s) => ({
        ...s,
        selectedFiles: new Set(),
        userDeselected: new Set(),
        commitToAmend: null,
      }))
      lastAmendSha = null
      onCommitted?.()
    } catch (err) {
      error = `${isAmending ? 'Amend' : 'Commit'} failed: ${String(err)}`
    } finally {
      isCommitting = false
    }
  }

  // A single-line <input> scrolls horizontally as the caret moves but ignores
  // wheel / trackpad gestures, so a long summary can't be swiped through. Map a
  // wheel delta onto scrollLeft (dominant axis, so both vertical scroll and a
  // horizontal swipe work) — only when there's actually overflow, and only then
  // do we preventDefault so we don't hijack the surrounding scroll otherwise.
  function handleSummaryWheel(e: WheelEvent) {
    const el = e.currentTarget as HTMLInputElement
    if (el.scrollWidth <= el.clientWidth) return
    const delta = Math.abs(e.deltaX) >= Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    if (delta === 0) return
    el.scrollLeft += delta
    e.preventDefault()
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

<div class="commit-message-container" role="form" aria-label="Commit message">
  {#if isAmending}
    <div class="amend-notice" role="status">
      <span class="amend-notice-text">
        Your changes will modify your <strong>most recent commit</strong>.
      </span>
      <button
        type="button"
        class="stop-amending-link"
        onclick={() => onStopAmending?.()}
        disabled={isCommitting}
      >
        Stop amending
      </button>
    </div>
  {/if}

  <div class="summary-section">
    <input
      id="summary-input"
      type="text"
      class="summary-input"
      placeholder={summaryPlaceholder}
      bind:value={summary}
      maxlength="200"
      disabled={isGenerating || isCommitting}
      onwheel={handleSummaryWheel}
      onkeydown={handleKeyDown}
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
      onkeydown={handleKeyDown}
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
      title={isAmending ? 'Amend commit (Ctrl+Enter)' : 'Commit (Ctrl+Enter)'}
      aria-label={isAmending ? 'Amend commit' : 'Commit'}
    >
      {#if isCommitting}
        {isAmending ? 'Amending…' : 'Committing…'}
      {:else}
        {isAmending ? 'Amend commit' : 'Commit'}
      {/if}
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
    height: 100%;
    min-height: 0;
    box-sizing: border-box;
  }

  .amend-notice {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 4px 8px;
    border-left: 2px solid var(--status-yellow);
    background: transparent;
    font-size: 11px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .amend-notice-text {
    flex: 1;
    min-width: 0;
  }

  .amend-notice-text strong {
    color: var(--text-primary);
    font-weight: 500;
  }

  .stop-amending-link {
    padding: 0;
    background: transparent;
    border: none;
    color: var(--border-active);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .stop-amending-link:hover:not(:disabled) {
    text-decoration: underline;
  }

  .stop-amending-link:disabled {
    color: var(--text-faint);
    cursor: not-allowed;
  }

  .summary-section {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .description-section {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
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
    flex: 1;
    min-height: 80px;
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

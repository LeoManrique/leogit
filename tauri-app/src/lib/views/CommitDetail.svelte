<script lang="ts">
  import type { CommitInfo } from '$lib/api/commands'

  interface Props {
    commit: CommitInfo | null
    fileCount?: number
  }

  let { commit = null, fileCount = 0 }: Props = $props()

  let copied = $state(false)

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString()
  }

  async function copySha() {
    if (!commit) return
    try {
      await navigator.clipboard.writeText(commit.sha)
      copied = true
      setTimeout(() => (copied = false), 1200)
    } catch {}
  }
</script>

{#if commit}
  <div class="commit-card">
    <h2 class="title">{commit.summary}</h2>

    {#if commit.body}
      <pre class="body">{commit.body}</pre>
    {/if}

    <div class="meta-row">
      <span class="author">{commit.author_name}</span>
      <span class="dot">·</span>
      <span class="email">{commit.author_email}</span>
      <span class="date">{formatDate(commit.author_date)}</span>
    </div>

    <div class="meta-row">
      <code class="sha">{commit.sha}</code>
      <button class="copy-btn" onclick={copySha} title="Copy SHA" aria-label="Copy SHA">
        {copied ? '✓' : '⎘'}
      </button>
      {#if fileCount > 0}
        <span class="files-count">{fileCount} {fileCount === 1 ? 'file' : 'files'} changed</span>
      {/if}
    </div>

    {#if commit.trailers.length > 0}
      <div class="trailers">
        {#each commit.trailers as trailer}
          <code class="trailer">{trailer}</code>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .commit-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-primary);
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .body {
    margin: 0;
    padding: 8px 10px;
    background: var(--bg-secondary);
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-wrap: break-word;
    max-height: 140px;
    overflow-y: auto;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-muted);
    flex-wrap: wrap;
  }

  .author {
    color: var(--text-primary);
    font-weight: 500;
  }

  .email {
    color: var(--text-muted);
  }

  .dot {
    color: var(--text-faint);
  }

  .date {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    margin-left: auto;
  }

  .sha {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    background: transparent;
    word-break: break-all;
  }

  .copy-btn {
    padding: 1px 6px;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    line-height: 1;
  }

  .copy-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .files-count {
    margin-left: auto;
    color: var(--text-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .trailers {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .trailer {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    background: transparent;
  }
</style>

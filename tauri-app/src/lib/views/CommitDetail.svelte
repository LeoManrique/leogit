<script lang="ts">
  import type { CommitInfo, CommitStats } from '$lib/api/commands'

  interface Props {
    commit: CommitInfo | null
    fileCount?: number
    stats?: CommitStats | null
  }

  let { commit = null, fileCount = 0, stats = null }: Props = $props()

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
    <div class="title-row">
      <h2 class="title">{commit.summary}</h2>
      {#if stats && (stats.additions > 0 || stats.deletions > 0)}
        <span class="commit-counts">
          {#if stats.additions > 0}<span class="add-count">+{stats.additions}</span>{/if}
          {#if stats.deletions > 0}<span class="del-count">-{stats.deletions}</span>{/if}
        </span>
      {/if}
    </div>

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
      <button class="copy-btn" class:copied onclick={copySha} title={copied ? 'Copied' : 'Copy SHA'} aria-label="Copy SHA">
        {#if copied}
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <polyline points="3.5,8.5 6.5,11.5 12.5,5" />
          </svg>
        {:else}
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="5" y="5" width="8" height="8" rx="1.5" />
            <path d="M11 5V3.5A1 1 0 0 0 10 2.5H4A1 1 0 0 0 3 3.5V10A1 1 0 0 0 4 11H5" />
          </svg>
        {/if}
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

  .title-row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    min-width: 0;
  }

  /* Commit-level +adds/-dels, pinned to the top-right at the title's baseline. */
  .commit-counts {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    line-height: 1.4;
  }

  .add-count { color: var(--diff-add-fg); }
  .del-count { color: var(--diff-remove-fg); }

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
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: color 120ms ease, background 120ms ease;
  }

  .copy-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .copy-btn.copied {
    color: var(--status-green);
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

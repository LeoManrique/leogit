<script lang="ts">
  import type { CommitInfo, FileEntry } from '$lib/api/commands'

  interface Props {
    commit: CommitInfo | null
    files?: FileEntry[]
  }

  let { commit = null, files = [] }: Props = $props()

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString()
  }

  function getFileStatusColor(status: string): string {
    switch (status) {
      case 'New':
        return 'var(--status-green)'
      case 'Modified':
        return 'var(--status-yellow)'
      case 'Deleted':
        return 'var(--status-red)'
      case 'Renamed':
        return 'var(--status-blue)'
      case 'Conflicted':
        return 'var(--status-red)'
      default:
        return 'var(--text-secondary)'
    }
  }
</script>

<div class="commit-detail">
  {#if commit === null}
    <div class="empty-state">
      <p>Select a commit to view details</p>
    </div>
  {:else}
    <div class="detail-content">
      <!-- SHA Section -->
      <div class="sha-section">
        <div class="sha-row">
          <span class="label">SHA</span>
          <code class="sha-value">{commit.sha}</code>
        </div>
        <div class="short-sha-row">
          <span class="label">Short</span>
          <code class="short-sha-value">{commit.short_sha}</code>
        </div>
      </div>

      <!-- Summary Section -->
      <div class="summary-section">
        <h2 class="summary-title">{commit.summary}</h2>
      </div>

      <!-- Metadata Section -->
      <div class="metadata-section">
        <div class="metadata-row">
          <div class="metadata-left">
            <div class="author-info">
              <div class="author-name">{commit.author_name}</div>
              <div class="author-email">{commit.author_email}</div>
            </div>
          </div>
          <div class="metadata-right">
            <div class="date-info">{formatDate(commit.author_date)}</div>
          </div>
        </div>
      </div>

      <!-- Body Section -->
      {#if commit.body}
        <div class="body-section">
          <h3 class="section-title">Message</h3>
          <pre class="body-text">{commit.body}</pre>
        </div>
      {/if}

      <!-- Trailers Section -->
      {#if commit.trailers.length > 0}
        <div class="trailers-section">
          <h3 class="section-title">Trailers</h3>
          <div class="trailers-list">
            {#each commit.trailers as trailer}
              <div class="trailer-line">
                <code>{trailer}</code>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Files Section -->
      {#if files.length > 0}
        <div class="files-section">
          <h3 class="section-title">Files Changed ({files.length})</h3>
          <div class="files-list">
            {#each files as file}
              <div class="file-entry">
                <span class="file-status" style="color: {getFileStatusColor(file.status)}">
                  {file.status.charAt(0)}
                </span>
                <span class="file-path">{file.display_name}</span>
                {#if file.display_dir}
                  <span class="file-dir">{file.display_dir}</span>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .commit-detail {
    flex: 1;
    overflow-y: auto;
    background: var(--bg-primary);
    display: flex;
    flex-direction: column;
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
  }

  .detail-content {
    padding: 20px;
    max-width: 100%;
  }

  .sha-section {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
  }

  .sha-row,
  .short-sha-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label {
    font-size: 11px;
    color: var(--text-muted);
  }

  .sha-value,
  .short-sha-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
    word-break: break-all;
    background: transparent;
  }

  .summary-section {
    margin-bottom: 16px;
  }

  .summary-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .metadata-section {
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
  }

  .metadata-row {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  .metadata-left {
    flex: 1;
  }

  .metadata-right {
    flex: 0 0 auto;
    text-align: right;
  }

  .author-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .author-name {
    color: var(--text-primary);
    font-weight: 500;
    font-size: 13px;
  }

  .author-email {
    font-size: 11px;
    color: var(--text-muted);
  }

  .date-info {
    color: var(--text-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .body-section {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 6px 0;
  }

  .body-text {
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-wrap: break-word;
    margin: 0;
    max-height: 300px;
    overflow-y: auto;
  }

  .trailers-section {
    margin-bottom: 16px;
  }

  .trailers-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .trailer-line {
    padding: 4px 12px;
    background: var(--bg-secondary);
    border-radius: 6px;
    font-size: 12px;
  }

  .trailer-line code {
    font-family: var(--font-mono);
    color: var(--text-muted);
    background: transparent;
  }

  .files-section {
    margin-bottom: 16px;
  }

  .files-list {
    display: flex;
    flex-direction: column;
  }

  .file-entry {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 3px 8px;
    min-height: 24px;
    border-radius: 6px;
    font-size: 13px;
  }

  .file-entry:hover {
    background: var(--surface-hover);
  }

  .file-status {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    width: 14px;
    text-align: center;
  }

  .file-path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
  }

  .file-dir {
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
  }
</style>

<script lang="ts">
  import type { FileEntry } from '$lib/api/commands'

  interface Props {
    files: FileEntry[]
    selectedFiles: Set<string>
    activeFile?: FileEntry | null
    onActivate: (file: FileEntry) => void
    onToggle: (file: FileEntry) => void
  }

  let { files = [], selectedFiles = new Set(), activeFile = null, onActivate, onToggle }: Props = $props()

  function getStatusColor(status: string): string {
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

  function getStatusLabel(status: string): string {
    switch (status) {
      case 'New':
        return 'A'
      case 'Modified':
        return 'M'
      case 'Deleted':
        return 'D'
      case 'Renamed':
        return 'R'
      case 'Conflicted':
        return 'U'
      default:
        return '?'
    }
  }

  function handleCheckboxChange(e: Event, file: FileEntry) {
    e.stopPropagation()
    onToggle(file)
  }
</script>

<div class="file-list">
  {#each files as file (file.path)}
    {@const isSelected = selectedFiles.has(file.path)}
    {@const isActive = activeFile?.path === file.path}
    <div
      class="file-row"
      class:active={isActive}
      class:selected={isSelected}
      onclick={() => onActivate(file)}
      role="button"
      tabindex="0"
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          onActivate(file)
        } else if (e.key === ' ') {
          e.preventDefault()
          onToggle(file)
        }
      }}
    >
      <input
        type="checkbox"
        class="file-checkbox"
        checked={isSelected}
        aria-label={isSelected ? `Exclude ${file.path} from commit` : `Include ${file.path} in commit`}
        onclick={(e) => e.stopPropagation()}
        onchange={(e) => handleCheckboxChange(e, file)}
        onkeydown={(e) => e.stopPropagation()}
      />

      <div class="status-badge" style="color: {getStatusColor(file.status)}">
        {getStatusLabel(file.status)}
      </div>

      <div class="file-info">
        <div class="file-name">
          {#if file.orig_path}
            <span class="orig">{file.orig_path}</span>
            <span class="arrow">→</span>
          {/if}
          {file.display_name}
        </div>
        {#if file.display_dir}
          <div class="file-dir">{file.display_dir}</div>
        {/if}
      </div>
    </div>
  {/each}

  {#if files.length === 0}
    <div class="empty-state">
      <p>No changes</p>
    </div>
  {/if}
</div>

<style>
  .file-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    background: var(--bg-secondary);
    padding: 4px 6px;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-faint);
    font-size: 13px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    min-height: 24px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 100ms ease;
  }

  .file-row:hover {
    background: var(--surface-hover);
  }

  .file-row.active {
    background: var(--bg-tertiary);
  }

  .file-checkbox {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    cursor: pointer;
    accent-color: var(--border-active);
    margin: 0;
  }

  .file-checkbox:focus-visible {
    outline: 2px solid var(--border-active);
    outline-offset: 2px;
  }

  .status-badge {
    width: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .file-info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .file-name {
    color: var(--text-primary);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-row.selected .file-name {
    font-weight: 500;
  }

  .orig {
    color: var(--text-muted);
    text-decoration: line-through;
    margin-right: 4px;
  }

  .arrow {
    color: var(--text-muted);
    margin-right: 4px;
  }

  .file-dir {
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

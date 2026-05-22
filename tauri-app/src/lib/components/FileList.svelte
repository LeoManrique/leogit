<script lang="ts">
  import type { FileEntry } from '$lib/api/commands'
  import PathText from './PathText.svelte'

  interface Props {
    files: FileEntry[]
    selectedFiles?: Set<string>
    activeFile?: FileEntry | null
    showCheckbox?: boolean
    onActivate: (file: FileEntry) => void
    onToggle?: (file: FileEntry) => void
  }

  let {
    files = [],
    selectedFiles = new Set(),
    activeFile = null,
    showCheckbox = true,
    onActivate,
    onToggle = () => {},
  }: Props = $props()

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
        } else if (e.key === ' ' && showCheckbox) {
          e.preventDefault()
          onToggle(file)
        }
      }}
    >
      {#if showCheckbox}
        <input
          type="checkbox"
          class="file-checkbox"
          checked={isSelected}
          aria-label={isSelected ? `Exclude ${file.path} from commit` : `Include ${file.path} in commit`}
          onclick={(e) => e.stopPropagation()}
          onchange={(e) => handleCheckboxChange(e, file)}
          onkeydown={(e) => e.stopPropagation()}
        />
      {/if}

      <div class="status-badge" style="color: {getStatusColor(file.status)}">
        {getStatusLabel(file.status)}
      </div>

      {#if file.orig_path}
        <div class="file-info" title={file.path}>
          <span class="orig">{file.orig_path}</span>
          <span class="arrow">→</span>
          <PathText path={file.path} />
        </div>
      {:else}
        <PathText path={file.path} />
      {/if}
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
    overflow-x: hidden;
    scrollbar-gutter: stable;
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
    padding: 0 8px;
    height: 24px;
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
    align-items: baseline;
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    white-space: nowrap;
    gap: 4px;
  }

  .orig {
    color: var(--text-muted);
    text-decoration: line-through;
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .arrow {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .file-row.selected :global(.filename) {
    font-weight: 500;
  }
</style>

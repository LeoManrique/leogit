<script lang="ts">
  import type { BranchInfo } from '$lib/api/commands'

  interface Props {
    branches: BranchInfo[]
    currentBranch: string
    onSwitch: (branch: string) => void
    onCreate: (name: string) => void
    onDelete: (name: string) => void
  }

  let { branches = [], currentBranch = '', onSwitch, onCreate, onDelete }: Props = $props()

  type Mode = 'browse' | 'create' | 'delete'

  let mode = $state<Mode>('browse')
  let newBranchName = $state('')
  let deleteBranchName = $state<string | null>(null)

  function handleSwitch(branch: string) {
    onSwitch(branch)
    mode = 'browse'
  }

  function handleCreateClick() {
    mode = 'create'
    newBranchName = ''
  }

  function handleCreateSubmit() {
    if (newBranchName.trim()) {
      onCreate(newBranchName.trim())
      newBranchName = ''
      mode = 'browse'
    }
  }

  function handleDeleteClick(branchName: string) {
    deleteBranchName = branchName
    mode = 'delete'
  }

  function handleDeleteConfirm() {
    if (deleteBranchName) {
      onDelete(deleteBranchName)
      deleteBranchName = null
      mode = 'browse'
    }
  }

  function handleCancel() {
    mode = 'browse'
    newBranchName = ''
    deleteBranchName = null
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      if (mode === 'create') {
        handleCreateSubmit()
      }
    } else if (e.key === 'Escape') {
      handleCancel()
    }
  }

  const localBranches = $derived(branches.filter((b) => !b.is_remote))
  const remoteBranches = $derived(branches.filter((b) => b.is_remote))
</script>

<div class="branch-dropdown">
  {#if mode === 'browse'}
    <div class="branch-list">
      <div class="section">
        <h3 class="section-title">Local Branches</h3>
        <div class="branches">
          {#each localBranches as branch}
            <div class="branch-item-wrapper">
              <button
                class="branch-item"
                class:current={branch.name === currentBranch}
                onclick={() => handleSwitch(branch.name)}
              >
                <span class="branch-name">{branch.name}</span>
                {#if branch.name === currentBranch}
                  <span class="current-badge">●</span>
                {/if}
              </button>
              {#if branch.name !== currentBranch}
                <button class="delete-btn" onclick={() => handleDeleteClick(branch.name)} title="Delete">
                  ✕
                </button>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      {#if remoteBranches.length > 0}
        <div class="section">
          <h3 class="section-title">Remote Branches</h3>
          <div class="branches">
            {#each remoteBranches as branch}
              <button
                class="branch-item"
                class:remote={true}
                onclick={() => handleSwitch(branch.name)}
              >
                <span class="branch-name">{branch.name}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="section-footer">
        <button class="create-btn" onclick={handleCreateClick}>+ New Branch</button>
      </div>
    </div>
  {:else if mode === 'create'}
    <div class="create-form">
      <h3>Create New Branch</h3>
      <input
        type="text"
        class="branch-input"
        placeholder="Branch name"
        bind:value={newBranchName}
        onkeydown={handleKeyDown}
        autofocus
      />
      <div class="form-buttons">
        <button class="btn-primary" onclick={handleCreateSubmit}>Create</button>
        <button class="btn-secondary" onclick={handleCancel}>Cancel</button>
      </div>
    </div>
  {:else if mode === 'delete'}
    <div class="delete-confirm">
      <h3>Delete Branch?</h3>
      <p>Are you sure you want to delete <code>{deleteBranchName}</code>?</p>
      <div class="form-buttons">
        <button class="btn-danger" onclick={handleDeleteConfirm}>Delete</button>
        <button class="btn-secondary" onclick={handleCancel}>Cancel</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .branch-dropdown {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    min-width: 280px;
    max-height: 420px;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .branch-list {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .section {
    padding: 6px 4px;
  }

  .section:not(:last-child) {
    border-bottom: 1px solid var(--border-inactive);
  }

  .section-title {
    padding: 4px 12px 6px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-muted);
    margin: 0;
  }

  .branches {
    display: flex;
    flex-direction: column;
  }

  .branch-item-wrapper {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0 4px;
  }

  .branch-item {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    min-height: 24px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 13px;
    text-align: left;
    border-radius: 6px;
    transition: background 100ms ease;
  }

  .branch-item:hover {
    background: var(--surface-hover);
  }

  .branch-item.current {
    background: var(--bg-tertiary);
    font-weight: 500;
  }

  .branch-item.remote {
    color: var(--text-muted);
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .current-badge {
    color: var(--border-active);
    margin-left: 8px;
    font-size: 8px;
  }

  .delete-btn {
    padding: 2px 6px;
    margin-right: 4px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    transition: background 100ms ease, color 100ms ease;
    opacity: 0;
  }

  .branch-item-wrapper:hover .delete-btn {
    opacity: 1;
  }

  .delete-btn:hover {
    background: var(--surface-hover);
    color: var(--status-red);
  }

  .section-footer {
    padding: 6px 8px 8px;
    border-top: 1px solid var(--border-inactive);
  }

  .create-btn {
    width: 100%;
    padding: 4px 10px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    text-align: left;
    transition: background 100ms ease, color 100ms ease;
  }

  .create-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .create-form,
  .delete-confirm {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .create-form h3,
  .delete-confirm h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .delete-confirm p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .delete-confirm code {
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
  }

  .branch-input {
    padding: 4px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 13px;
  }

  .branch-input:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  .form-buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn-primary,
  .btn-secondary,
  .btn-danger {
    padding: 3px 14px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    transition: background 120ms ease, border-color 120ms ease;
  }

  .btn-primary {
    background: var(--border-active);
    color: #ffffff;
    border-color: var(--border-active);
  }

  .btn-primary:hover {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background: var(--surface-hover);
  }

  .btn-danger {
    background: var(--bg-elevated);
    color: var(--status-red);
  }

  .btn-danger:hover {
    background: var(--surface-hover);
  }
</style>

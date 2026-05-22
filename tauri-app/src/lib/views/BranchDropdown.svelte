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
    background: var(--bg-secondary);
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    min-width: 250px;
    max-height: 400px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1);
  }

  .branch-list {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .section {
    padding: 8px 0;
  }

  .section:not(:last-child) {
    border-bottom: 1px solid var(--border-inactive);
  }

  .section-title {
    padding: 4px 12px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
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
  }

  .branch-item {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 13px;
    text-align: left;
    transition: background 150ms ease;
  }

  .branch-item:hover {
    background: var(--bg-tertiary);
  }

  .branch-item.current {
    background: var(--cursor-bg);
    font-weight: 500;
  }

  .branch-item.remote {
    color: var(--text-secondary);
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', 'Courier New', monospace;
    font-size: 12px;
  }

  .current-badge {
    color: var(--status-green);
    margin-left: 8px;
    font-size: 10px;
  }

  .delete-btn {
    padding: 4px 8px;
    margin-right: 4px;
    background: transparent;
    border: 1px solid var(--border-inactive);
    color: var(--text-secondary);
    border-radius: 3px;
    cursor: pointer;
    font-size: 12px;
    transition: all 150ms ease;
    opacity: 0;
  }

  .branch-item-wrapper:hover .delete-btn {
    opacity: 1;
  }

  .delete-btn:hover {
    background: var(--status-red);
    border-color: var(--status-red);
    color: white;
  }

  .section-footer {
    padding: 8px 12px;
    border-top: 1px solid var(--border-inactive);
  }

  .create-btn {
    width: 100%;
    padding: 8px 12px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-inactive);
    color: var(--text-primary);
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    transition: all 150ms ease;
  }

  .create-btn:hover {
    background: var(--border-inactive);
  }

  .create-form,
  .delete-confirm {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .create-form h3,
  .delete-confirm h3 {
    margin: 0;
    font-size: 14px;
    color: var(--text-primary);
  }

  .delete-confirm p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .delete-confirm code {
    background: transparent;
    color: var(--status-blue);
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', 'Courier New', monospace;
  }

  .branch-input {
    padding: 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-inactive);
    border-radius: 4px;
    color: var(--text-primary);
    font-size: 13px;
  }

  .branch-input:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 3px var(--cursor-bg);
  }

  .form-buttons {
    display: flex;
    gap: 8px;
  }

  .btn-primary,
  .btn-secondary,
  .btn-danger {
    flex: 1;
    padding: 8px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    transition: all 150ms ease;
  }

  .btn-primary {
    background: var(--status-green);
    color: white;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-inactive);
  }

  .btn-secondary:hover {
    background: var(--border-inactive);
  }

  .btn-danger {
    background: var(--status-red);
    color: white;
  }

  .btn-danger:hover {
    opacity: 0.9;
  }
</style>

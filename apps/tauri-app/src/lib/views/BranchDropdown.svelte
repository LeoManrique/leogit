<script lang="ts">
  import type { BranchInfo } from '$lib/api/commands'
  import { autofocus } from '$lib/actions/autofocus'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import { nextActiveIndex, scrollIntoViewWhenActive } from '$lib/actions/listNavigation'
  import ContextMenu, { type ContextMenuItem } from '$lib/components/ContextMenu.svelte'
  import Icon from '$lib/components/Icon.svelte'

  interface Props {
    branches: BranchInfo[]
    currentBranch: string
    /** HEAD is on a commit, not a branch: there is no target to merge into. */
    detached: boolean
    /** A merge is in progress — the only branch action that makes sense is
     *  aborting it. */
    merging: boolean
    /** A branch operation is in flight; every action here locks until it ends. */
    busy: boolean
    onSwitch: (branch: string) => void
    /**
     * Create and switch. Resolves to core's failure text, or undefined on
     * success: the form keeps the typed name and states the failure under the
     * field, because the field is where the fix is (FRONTEND §6.13).
     */
    onCreate: (name: string) => Promise<string | undefined>
    /** Open the merge dialog for `source` → the current branch. */
    onRequestMerge: (source: string) => void
    /** Open the delete confirmation for a local branch. */
    onRequestDelete: (name: string) => void
    /** Open the abort-merge confirmation. */
    onRequestAbortMerge: () => void
    /** Dismiss the popover. Registered on the overlay stack, so Escape reaches
     *  it wherever focus happens to be. */
    onClose: () => void
  }

  let {
    branches = [],
    currentBranch = '',
    detached = false,
    merging = false,
    busy = false,
    onSwitch,
    onCreate,
    onRequestMerge,
    onRequestDelete,
    onRequestAbortMerge,
    onClose,
  }: Props = $props()

  /*
    The popover is the branch *menu*, not just a switcher — the same four
    actions the native client's menu carries. Two of them need a branch as an
    argument, and rather than inventing a second list for each, they put this
    one into a picking mode: the header says which question is being asked and
    the rows answer it. That is what the native submenus are, and it means the
    keyboard cursor, the filter and the row rendering are written once.
  */
  type Mode = 'browse' | 'create' | 'merge' | 'delete'

  let mode = $state<Mode>('browse')
  const isPicking = $derived(mode === 'merge' || mode === 'delete')

  let filter = $state('')
  let newBranchName = $state('')
  /** A failed create, stated under the field with the name still in it. */
  let createError = $state<string | undefined>(undefined)
  let rowMenu = $state<{ branch: BranchInfo; x: number; y: number } | null>(null)

  const localBranches = $derived(branches.filter((b) => !b.is_remote))

  /** Anything but the branch you are on can be merged into it, remotes included. */
  const mergeCandidates = $derived(branches.filter((b) => b.name !== currentBranch))
  /** Only local, non-current branches are deletable. */
  const deleteCandidates = $derived(localBranches.filter((b) => b.name !== currentBranch))

  const canMerge = $derived(!detached && !merging && mergeCandidates.length > 0)
  const canDelete = $derived(deleteCandidates.length > 0)

  const mergeHelp = $derived(
    detached
      ? 'Detached HEAD — check out a branch before merging into it'
      : merging
        ? 'Finish or abort the merge in progress first'
        : mergeCandidates.length === 0
          ? 'There is no other branch to merge from'
          : `Merge another branch into ${currentBranch}`,
  )

  /*
    Rows for the mode we are in, locals first so one flat index can carry the
    keyboard cursor across both sections. Deliberately a plain case-insensitive
    substring rather than core's repo matcher: that one is built for paths and
    scan roots, and a branch name is short enough that a contiguous match is
    the predictable answer.
  */
  const candidates = $derived(
    mode === 'merge' ? mergeCandidates : mode === 'delete' ? deleteCandidates : branches,
  )

  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    const rows = q ? candidates.filter((b) => b.name.toLowerCase().includes(q)) : candidates
    return [...rows].sort((a, b) => Number(a.is_remote) - Number(b.is_remote))
  })

  /** Where the "Remote Branches" heading goes; -1 when there are none. */
  const firstRemoteIndex = $derived(filtered.findIndex((b) => b.is_remote))

  // Keyboard cursor over the filtered rows, reset to the top match on every
  // keystroke and on every mode change — the same rule the repo pickers use, so
  // Return always acts on the row a query just put first.
  let activeIndex = $state(0)
  $effect(() => {
    filter
    mode
    activeIndex = 0
  })

  function activate(branch: BranchInfo) {
    if (busy) return
    if (mode === 'merge') {
      onRequestMerge(branch.name)
    } else if (mode === 'delete') {
      onRequestDelete(branch.name)
    } else if (branch.name !== currentBranch) {
      // The current branch is not a target: checking out the branch you are
      // already on spends a checkout plus a full refresh chain to arrive
      // exactly where you started.
      onSwitch(branch.name)
    }
  }

  function startPicking(next: 'merge' | 'delete') {
    if (busy) return
    filter = ''
    mode = next
  }

  function backToBrowse() {
    filter = ''
    createError = undefined
    mode = 'browse'
  }

  async function submitCreate() {
    const name = newBranchName.trim()
    if (!name || busy) return
    createError = undefined
    const failure = await onCreate(name)
    // A rejected name — already taken, or not a legal ref — is corrected right
    // here. Clearing the field before the outcome was how a typo cost the whole
    // name and dropped the user back on a closed dropdown with a modal over it.
    if (failure) {
      createError = failure
      return
    }
    newBranchName = ''
    backToBrowse()
  }

  function handleListKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filtered.length, 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      activeIndex = nextActiveIndex(activeIndex, filtered.length, -1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const branch = filtered[activeIndex]
      if (branch) activate(branch)
    }
  }

  function handleCreateKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      void submitCreate()
    }
  }

  /**
   * Escape leaves the sub-question before it leaves the popover: the create
   * form and the two picking modes are steps *inside* this surface, and closing
   * the whole thing is one step too many when the user only meant to back out
   * of one. From browse there is nothing left to back out of, so it closes.
   */
  function escape(): void {
    if (mode === 'browse') onClose()
    else backToBrowse()
  }

  /*
    The row's own actions, for the pointer. The list already reaches them by
    keyboard through the footer's picking modes; this is the mouse's short path
    to the same two, and the natural home for a rename when that lands. It
    replaces the hover-only ✕, which no keyboard user could see and which put
    the one destructive action on the row's most casual gesture.
  */
  function openRowMenu(e: MouseEvent, branch: BranchInfo) {
    // Suppressed in every mode, opened only in the one that has actions to
    // offer — otherwise a right-click in a picking mode falls through to the
    // webview's own menu, which knows nothing about branches.
    e.preventDefault()
    if (mode !== 'browse' || busy) return
    rowMenu = { branch, x: e.clientX, y: e.clientY }
  }

  // Destructive first behind a divider, then the repository-changing pair —
  // STYLE.md's ordering, so the item that can lose work never sits next to the
  // one people click most. Items that don't apply to this row are disabled
  // rather than dropped, so the menu keeps one shape.
  const rowMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const branch = rowMenu?.branch
    if (!branch) return []
    const isCurrent = branch.name === currentBranch
    return [
      {
        label: 'Delete…',
        action: () => onRequestDelete(branch.name),
        enabled: !branch.is_remote && !isCurrent,
        destructive: true,
      },
      { separator: true, label: '', action: () => {} },
      {
        label: 'Switch to Branch',
        action: () => onSwitch(branch.name),
        enabled: !isCurrent,
      },
      {
        label: `Merge into “${currentBranch}”…`,
        action: () => onRequestMerge(branch.name),
        enabled: canMerge && !isCurrent,
      },
    ]
  })
</script>

<div class="branch-dropdown" use:dismissOnEscape={escape}>
  {#if mode === 'create'}
    <div class="create-form">
      <h3>Create New Branch</h3>
      <input
        type="text"
        class="text-input"
        placeholder="Branch name"
        bind:value={newBranchName}
        onkeydown={handleCreateKeyDown}
        disabled={busy}
        use:autofocus
      />
      {#if createError}
        <p class="error">{createError}</p>
      {/if}
      <div class="form-buttons">
        <button class="btn-secondary" onclick={backToBrowse} disabled={busy}>Cancel</button>
        <button
          class="btn-primary"
          onclick={submitCreate}
          disabled={busy || newBranchName.trim() === ''}
        >
          {busy ? 'Creating…' : 'Create Branch'}
        </button>
      </div>
    </div>
  {:else}
    {#if isPicking}
      <div class="pick-header">
        <button class="back-btn" onclick={backToBrowse} aria-label="Back to branches">
          <Icon name="chevron-left" weight="medium" />
        </button>
        <span class="pick-title">
          {#if mode === 'merge'}Merge into “{currentBranch}” — pick a branch{:else}Delete which
            branch?{/if}
        </span>
      </div>
    {/if}

    <!-- Keyed on the mode so entering or leaving a picking mode remounts the
         field and `use:autofocus` puts the caret back: the footer button that
         switched modes was holding focus, and the footer is gone the moment it
         is pressed — leaving the arrow keys with nothing listening. -->
    {#key mode}
      <div class="filter-row">
        <input
          type="text"
          class="text-input"
          placeholder="Filter branches"
          bind:value={filter}
          onkeydown={handleListKeyDown}
          use:autofocus
        />
      </div>
    {/key}

    <div class="branch-list">
      {#if filtered.length === 0}
        <p class="empty">
          {#if filter.trim()}No branch matches “{filter.trim()}”.{:else}No branches here.{/if}
        </p>
      {:else}
        {#each filtered as branch, i (branch.name)}
          {#if i === 0 && !branch.is_remote}
            <h3 class="section-title">Local Branches</h3>
          {/if}
          {#if i === firstRemoteIndex}
            <h3 class="section-title">Remote Branches</h3>
          {/if}
          <button
            class="branch-item"
            class:active={i === activeIndex}
            class:destructive={mode === 'delete'}
            aria-current={branch.name === currentBranch ? 'true' : undefined}
            use:scrollIntoViewWhenActive={i === activeIndex}
            onclick={() => activate(branch)}
            oncontextmenu={(e) => openRowMenu(e, branch)}
            disabled={busy}
          >
            <!-- The menu's own checkmark: the native `Picker(.inline)` marks
                 the current branch with one, and the column is on every row
                 so the names line up. -->
            <span class="check" class:visible={branch.name === currentBranch} aria-hidden="true">
              <Icon name="checkmark" size={10} weight="semibold" />
            </span>
            <span class="branch-name">{branch.name}</span>
          </button>
        {/each}
      {/if}
    </div>

    {#if mode === 'browse'}
      <!--
        The branch menu's actions, in the order the native menu carries them.
        Merge and Delete need a branch, so they hand the list above the
        question instead of opening a second one. Abort appears only while a
        merge is in progress: it is the one action that has no meaning outside
        that state, and a permanently greyed row would be noise in every other
        repository.
      -->
      <div class="footer">
        <button class="footer-btn" onclick={() => (mode = 'create')} disabled={busy}>
          New Branch…
        </button>
        <button
          class="footer-btn"
          onclick={() => startPicking('merge')}
          disabled={busy || !canMerge}
          title={mergeHelp}
        >
          Merge into “{currentBranch || 'this branch'}”…
        </button>
        {#if merging}
          <button class="footer-btn destructive" onclick={onRequestAbortMerge} disabled={busy}>
            Abort Merge…
          </button>
        {/if}
        <button
          class="footer-btn"
          onclick={() => startPicking('delete')}
          disabled={busy || !canDelete}
          title={canDelete ? 'Delete a local branch' : 'There is no other local branch to delete'}
        >
          Delete Branch…
        </button>
      </div>
    {/if}
  {/if}
</div>

{#if rowMenu}
  <ContextMenu
    x={rowMenu.x}
    y={rowMenu.y}
    items={rowMenuItems}
    onClose={() => (rowMenu = null)}
  />
{/if}

<style>
  .branch-dropdown {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    /* The frame that hangs this under the chip owns the width and hands down
       how much room the chip leaves below it — see `RepoDropdown`. */
    width: 100%;
    max-height: min(440px, var(--popover-max-height, 440px));
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  /* Which question the list is answering, when it is not the default one. */
  .pick-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 6px 6px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    flex: 0 0 auto;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background 100ms ease,
      color 100ms ease;
  }

  .back-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .pick-title {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .filter-row {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .text-input {
    width: 100%;
    padding: 4px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text-primary);
    font-family: inherit;
    font-size: 13px;
  }

  .text-input:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  .text-input:disabled {
    opacity: 0.6;
  }

  .branch-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }

  .section-title {
    padding: 6px 10px 4px;
    flex-shrink: 0;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-muted);
    margin: 0;
  }

  .empty {
    margin: 0;
    padding: 14px 12px;
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
  }

  /* The repo picker's row, to the pixel: 26px (the native's 5 + 16 + 5), a
     14px checkmark column, and `flex-shrink: 0` so a repository with more
     branches than fit does not collapse every row to its text line — a
     fixed-height item in a scrolling flex column shrinks before the column
     overflows. Remote rows take the same primary as local ones: the native
     menu draws them as plain items, and muting them read as "disabled". */
  .branch-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 26px;
    flex-shrink: 0;
    background: transparent;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-family: inherit;
    font-size: 13px;
    text-align: left;
    border-radius: 6px;
    transition: background 100ms ease;
  }

  /* Hover and the keyboard cursor: one colour at two alphas, the native
     row's own device, so the two stay tellable apart. */
  .branch-item:hover:not(:disabled) {
    background: var(--selection-hover);
  }

  /* The hover rule above outranks a bare `.active` (three simple selectors to
     two) and would downgrade the cursor row to the hover wash the moment the
     pointer rested on it — the one confusion the two alphas exist to prevent.
     The hovered form is named so the cursor wins on its own row. */
  .branch-item.active,
  .branch-item.active:hover {
    background: var(--selection-cursor);
  }

  /* While picking what to delete, the rows are what the destructive action
     lands on, and they say so. */
  .branch-item.destructive {
    color: var(--status-red);
  }

  .branch-item:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .branch-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The current-branch column: on every row so the names line up, visible on
     the one the checkmark belongs to. The one marker a row gets. */
  .check {
    flex: 0 0 14px;
    display: inline-flex;
    justify-content: center;
    opacity: 0;
  }

  .check.visible {
    opacity: 1;
  }

  /* Actions sit outside the scrolling list, so they survive an empty one — the
     state where "create a branch" is most likely to be what you wanted. */
  .footer {
    display: flex;
    flex-direction: column;
    padding: 4px;
    border-top: 1px solid var(--border-inactive);
  }

  .footer-btn {
    width: 100%;
    padding: 0 10px;
    height: 26px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-secondary);
    cursor: pointer;
    font-family: inherit;
    font-size: 13px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition:
      background 100ms ease,
      color 100ms ease;
  }

  .footer-btn:hover:not(:disabled) {
    background: var(--selection-hover);
    color: var(--text-primary);
  }

  .footer-btn.destructive {
    color: var(--status-red);
  }

  .footer-btn:disabled {
    color: var(--text-faint);
    cursor: default;
  }

  .create-form {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .create-form h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  /* git's own refusal — a name already taken, a name it won't accept — kept
     selectable beside the field that has to change. */
  .create-form .error {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--status-red);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    max-height: 96px;
    overflow-y: auto;
  }

  .form-buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn-primary,
  .btn-secondary {
    padding: 3px 14px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    transition:
      background 120ms ease,
      border-color 120ms ease;
  }

  .btn-primary {
    background: var(--border-active);
    color: var(--on-accent);
    border-color: var(--border-active);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn-primary:disabled,
  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>

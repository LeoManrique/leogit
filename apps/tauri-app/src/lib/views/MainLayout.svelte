<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { get } from 'svelte/store'
  import { listen } from '@tauri-apps/api/event'
  import {
    repoState,
    resetRepoState,
    setActiveTab,
    reportActionError,
    reportNotice,
    dismissActionError,
    dismissNotice,
  } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { dismissTopOverlay, overlayDepth } from '$lib/actions/overlayStack'
  import { config, patchConfig, refreshConfig } from '$lib/stores/config'
  import { hydrateReposState, patchReposState, recordRecentRepo } from '$lib/stores/reposState'
  import { setRepoSync } from '$lib/stores/repoSync'
  import { activeNetworkOp } from '$lib/stores/networkOps'
  import { repoSyncScheduler } from '$lib/services/repoSyncScheduler'
  import { rediscoverRepos } from '$lib/services/repoDiscovery'
  import { resolveCloneDefaultDir, rememberCloneDir } from '$lib/services/cloneFlow'
  import {
    shouldAttemptBackground,
    recordResult,
    initConnectivity,
  } from '$lib/services/connectivity'
  import {
    gitApi,
    diffApi,
    exclusionsApi,
    webviewDiffOptions,
    type Exclusion,
    type FileEntry,
    type CommitInfo,
    type Config,
    type DiffSizeGuard,
    type DiscardPlan,
    type EmptyDiffReason,
    type LaunchTarget,
    type MergeResult,
    type ParsedDiff,
    type RepoStatus as GitStatus,
  } from '$lib/api/commands'
  import * as fileActions from '$lib/services/fileActions'
  import type { FileContextActions } from '$lib/services/fileActions'
  import { isFromTerminal } from '$lib/utils/keyboard'
  import { isTextInputElement, isTextInputFocused } from '$lib/utils/focus'
  import {
    autoFetchIntervalMs as pacedFetchIntervalMs,
    canAutoFetch,
    canPollStatus,
    observeActivity,
    statusPollIntervalMs,
    wokeUp,
    SESSION_FETCH_SKEW_MS,
    type ActivityState,
  } from '$lib/services/backgroundPolicy'
  import { pacedLoop } from '$lib/services/pacedLoop'

  import Header from '$lib/components/Header.svelte'
  import Icon from '$lib/components/Icon.svelte'
  import PaneEmptyState from '$lib/components/PaneEmptyState.svelte'
  import TabBar from '$lib/components/TabBar.svelte'
  import FileList from '$lib/components/FileList.svelte'
  import DiscardConfirm from '$lib/components/DiscardConfirm.svelte'
  import CheckoutCommitConfirm from '$lib/components/CheckoutCommitConfirm.svelte'
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte'
  import MergeBranchDialog from '$lib/components/MergeBranchDialog.svelte'
  import CommitMessage from '$lib/components/CommitMessage.svelte'
  import CommitList from '$lib/components/CommitList.svelte'
  import DiffViewer from '$lib/components/DiffViewer.svelte'
  import SeamlessDiffPane from '$lib/components/SeamlessDiffPane.svelte'
  import Terminal from '$lib/components/Terminal.svelte'
  import CommitDetail from '$lib/views/CommitDetail.svelte'
  import BranchDropdown from '$lib/views/BranchDropdown.svelte'
  import RepoDropdown from '$lib/views/RepoDropdown.svelte'
  import CloneOverlay from '$lib/views/CloneOverlay.svelte'
  import SettingsOverlay from '$lib/views/SettingsOverlay.svelte'
  import HelpOverlay from '$lib/views/HelpOverlay.svelte'
  import ErrorModal from '$lib/components/ErrorModal.svelte'

  let terminalExpanded = $state(false)
  let terminalSessionId = $state(0) // 0 = no active PTY; >0 = key for the mounted Terminal
  // Shell the running session actually launched, reported by the backend. May
  // differ from the configured preference when that shell isn't installed, so
  // the header shows what's really running rather than what was asked for.
  let activeShellLabel = $state('')
  let showRepos = $state(false)
  let showClone = $state(false)
  // Destination pre-filled into the Clone dialog: last-used folder, else the
  // first configured scan path, else ~/Dev (the backend expands the leading ~).
  let cloneDefaultDir = $state('~/Dev')
  let showBranches = $state(false)
  let showSettings = $state(false)
  let showHelp = $state(false)

  // The header's two chips, bound out of it so each picker can hang from the
  // control that opens it. Measured when a picker opens rather than handed a
  // rect by the click, because ⌘B opens the branch menu with no click to
  // measure from — one anchoring path, not two. `null` once the chips unmount:
  // that is what `bind:this` writes on teardown, not `undefined`.
  let repoChip = $state<HTMLElement | null | undefined>()
  let branchChip = $state<HTMLElement | null | undefined>()
  // Bumped on resize so an open picker follows its chip, instead of staying at
  // the coordinates it opened at while the window resizes under it.
  let viewportVersion = $state(0)

  /*
    Where a picker hangs. The native repo chip presents an `NSPopover`
    (`RepoSwitcher.swift:44`): centred on the chip, an arrow pointing back at
    it, no scrim. The native branch chip is a pull-down `Menu`
    (`BranchMenu.swift:56`), which AppKit hangs from the control's leading edge
    with no arrow. Each surface here takes its counterpart's geometry — the
    branch popover's *shape* is a recorded divergence (FRONTEND.md §8); its
    placement never was.

    The arrow's numbers are `NSPopover`'s, which Apple does not publish. They
    were read off a live popover on macOS 26.6 (`NSPopoverFrame.anchorSize`
    27.5 × 13, the tip one point off the anchor, the box centred on it) and
    are kept here, in the one place that draws the arrow. The arrow is drawn
    as a turned square, so its base is twice its height: 26 against 27.5.
  */
  /** `RepoSwitcher.swift:68` declares 320 × 440. */
  const REPO_POPOVER_WIDTH = 320
  const BRANCH_POPOVER_WIDTH = 300
  const POPOVER_ARROW_HEIGHT = 13
  const POPOVER_ARROW_WIDTH = POPOVER_ARROW_HEIGHT * 2
  /** Chip bottom edge → arrow tip. */
  const POPOVER_GAP = 1
  /** Chip bottom edge → menu top edge. */
  const MENU_GAP = 4
  /** The least a box may sit from the window's edge. */
  const VIEWPORT_MARGIN = 8
  /** The arrow may not ride onto the box's 10px rounded corner. */
  const ARROW_CORNER_INSET = 10 + POPOVER_ARROW_WIDTH / 2

  type Placement = {
    left: number
    top: number
    /** The chip's centre in the frame's coordinates; `null` for a menu. */
    arrowX: number | null
    maxHeight: number
  }

  function placeUnder(
    chip: HTMLElement | null | undefined,
    width: number,
    arrowed: boolean,
  ): Placement | null {
    if (!chip) return null
    const rect = chip.getBoundingClientRect()
    const centre = rect.left + rect.width / 2
    const top = rect.bottom + (arrowed ? POPOVER_GAP + POPOVER_ARROW_HEIGHT : MENU_GAP)
    const wanted = arrowed ? centre - width / 2 : rect.left
    const rightmost = Math.max(VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN)
    const left = Math.min(Math.max(wanted, VIEWPORT_MARGIN), rightmost)
    // The arrow stays on the chip even when the window's edge pushed the box
    // off centre — it points at what was clicked, not at its own middle.
    const arrowX = arrowed
      ? Math.min(Math.max(centre - left, ARROW_CORNER_INSET), width - ARROW_CORNER_INSET)
      : null
    // Floored so a window shorter than its own header still gets a box rather
    // than a zero-height one.
    const maxHeight = Math.max(120, window.innerHeight - top - VIEWPORT_MARGIN)
    return { left, top, arrowX, maxHeight }
  }

  const repoPlacement = $derived.by(() => {
    void viewportVersion
    return showRepos ? placeUnder(repoChip, REPO_POPOVER_WIDTH, true) : null
  })

  const branchPlacement = $derived.by(() => {
    void viewportVersion
    return showBranches ? placeUnder(branchChip, BRANCH_POPOVER_WIDTH, false) : null
  })

  /** The frame's inline style. With no placement it keeps only its width and
   *  falls back to the layer's centring. */
  function placementStyle(placement: Placement | null, width: number): string {
    if (!placement) return `width: ${width}px`
    const arrow =
      placement.arrowX === null
        ? ''
        : `; --popover-arrow-x: ${placement.arrowX}px; --popover-arrow-height: ${POPOVER_ARROW_HEIGHT}px`
    return (
      `left: ${placement.left}px; top: ${placement.top}px; width: ${width}px; ` +
      `--popover-max-height: ${placement.maxHeight}px${arrow}`
    )
  }

  // The mounted composer, so the window-level key handler can reach ⌘↩ / ⌘G
  // without the fields owning them (CommitMessage explains why).
  let composer = $state<{ requestCommit: () => void; requestGenerate: () => void } | null>(null)
  // The mounted terminal, so `runInTerminal` can hand it a command. Null
  // whenever no session is up, which that function starts.
  let terminal = $state<{ runCommand: (command: string) => void } | null>(null)

  let lastHeadSha: string | null = null

  // Defer the "Loading diff…" placeholder so sub-150 ms fetches swap the
  // diff in place with no flash — below that a swap still reads as instant.
  // If the fetch outlives the threshold, the viewer falls back to the spinner.
  const SLOW_DIFF_THRESHOLD_MS = 150
  let diffLoadingTimer: ReturnType<typeof setTimeout> | null = null
  let commitDiffLoadingTimer: ReturnType<typeof setTimeout> | null = null

  const PAGE_SIZE = 50
  /*
    How deep a *refresh* re-reads, however far the user has paged. The list
    itself is append-only from HEAD and grows on demand, but re-fetching
    thousands of rows every time HEAD moves would make a commit cost a `git log`
    over the whole history; past this depth the oldest rows are dropped instead
    and re-grow by scrolling. The bound sits at the far end of the list, away
    from HEAD, so `commits[0]` is the repository's HEAD by construction — which
    is the property that makes the rewriting actions' gate unambiguous rather
    than merely correct today. Native's `historyRefreshCap`; FRONTEND §6.8.
  */
  const MAX_COMMITS = 500

  const SIDEBAR_MIN = 280
  const SIDEBAR_MAX = 640
  const COMMIT_MIN = 180
  const COMMIT_MAX = 600
  // The least the file list keeps when the composer is at its tallest.
  const FILE_LIST_MIN = 80
  const COMMIT_FILES_MIN = 180
  const COMMIT_FILES_MAX = 600

  function loadStoredNumber(key: string, fallback: number, min: number, max: number): number {
    if (typeof window === 'undefined') return fallback
    const raw = window.localStorage.getItem(key)
    const n = raw ? parseInt(raw, 10) : NaN
    return Number.isFinite(n) && n >= min && n <= max ? n : fallback
  }

  let sidebarWidth = $state(loadStoredNumber('leogit:sidebarWidth', 320, SIDEBAR_MIN, SIDEBAR_MAX))
  let commitHeight = $state(loadStoredNumber('leogit:commitHeight', 220, COMMIT_MIN, COMMIT_MAX))
  let commitFilesWidth = $state(
    loadStoredNumber('leogit:commitFilesWidth', 280, COMMIT_FILES_MIN, COMMIT_FILES_MAX),
  )

  // Measured height of the space the file list and the composer share, so a
  // stored height taller than today's window can't push the Commit button out
  // of the clipped pane. Zero until the first layout.
  let tabPanesHeight = $state(0)

  /**
   * How tall the composer may be *right now*: the fixed ceiling, capped by what
   * fits above the list's floor. Capping the drag as well as the render is what
   * keeps the stored value within reach — otherwise a drag back down first
   * spends an invisible surplus while the divider sits still. Unmeasured means
   * uncapped, so the stored height doesn't flash through the minimum on the
   * first frame.
   */
  const commitMax = $derived(
    tabPanesHeight > 0
      ? Math.min(COMMIT_MAX, Math.max(COMMIT_MIN, tabPanesHeight - FILE_LIST_MIN))
      : COMMIT_MAX,
  )

  /**
   * The stored height clamped into today's cap. The stored value itself is left
   * alone, so a window that grows again gives the user their full height back
   * without a fresh drag.
   */
  const effectiveCommitHeight = $derived(Math.min(commitHeight, commitMax))

  function startSidebarResize(e: MouseEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = sidebarWidth
    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - startX
      sidebarWidth = Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, startWidth + delta))
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:sidebarWidth', String(sidebarWidth))
    }
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  function startCommitFilesResize(e: MouseEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = commitFilesWidth
    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - startX
      commitFilesWidth = Math.max(
        COMMIT_FILES_MIN,
        Math.min(COMMIT_FILES_MAX, startWidth + delta),
      )
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:commitFilesWidth', String(commitFilesWidth))
    }
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  function startCommitResize(e: MouseEvent) {
    e.preventDefault()
    const startY = e.clientY
    // From what is on screen, not from the stored value: with a clamp in play
    // the two differ, and starting from the stored one would make the divider
    // jump on the first pixel of the drag.
    const startHeight = effectiveCommitHeight
    function onMove(ev: MouseEvent) {
      const delta = startY - ev.clientY
      commitHeight = Math.max(COMMIT_MIN, Math.min(commitMax, startHeight + delta))
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      window.localStorage.setItem('leogit:commitHeight', String(commitHeight))
    }
    document.body.style.cursor = 'row-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  // Keyboard resizing for the splitter handles (role="separator", tabindex=0).
  // Arrow keys nudge by RESIZE_STEP px; Home/End jump to the min/max. This is
  // what makes the handles accessible splitters rather than mouse-only drag
  // targets. `axis` selects which arrow keys apply: a 'horizontal' splitter
  // (stacked panes) responds to Up/Down, a 'vertical' splitter (side-by-side
  // panes) to Left/Right.
  const RESIZE_STEP = 16

  function splitterKey(
    e: KeyboardEvent,
    axis: 'horizontal' | 'vertical',
    current: number,
    min: number,
    max: number,
    apply: (value: number) => void,
  ) {
    const decKey = axis === 'horizontal' ? 'ArrowDown' : 'ArrowLeft'
    const incKey = axis === 'horizontal' ? 'ArrowUp' : 'ArrowRight'
    let next = current
    if (e.key === decKey) next = current - RESIZE_STEP
    else if (e.key === incKey) next = current + RESIZE_STEP
    else if (e.key === 'Home') next = min
    else if (e.key === 'End') next = max
    else return
    e.preventDefault()
    apply(Math.max(min, Math.min(max, next)))
  }

  function handleSidebarKey(e: KeyboardEvent) {
    splitterKey(e, 'vertical', sidebarWidth, SIDEBAR_MIN, SIDEBAR_MAX, (v) => {
      sidebarWidth = v
      window.localStorage.setItem('leogit:sidebarWidth', String(v))
    })
  }

  function handleCommitKey(e: KeyboardEvent) {
    splitterKey(e, 'horizontal', effectiveCommitHeight, COMMIT_MIN, commitMax, (v) => {
      commitHeight = v
      window.localStorage.setItem('leogit:commitHeight', String(v))
    })
  }

  function handleCommitFilesKey(e: KeyboardEvent) {
    splitterKey(e, 'vertical', commitFilesWidth, COMMIT_FILES_MIN, COMMIT_FILES_MAX, (v) => {
      commitFilesWidth = v
      window.localStorage.setItem('leogit:commitFilesWidth', String(v))
    })
  }

  /*
    Whether a parsed diff has something to put on screen. Core answers the
    "why not" question itself now — `empty_reason` and `size_guard` — so this
    is only the boolean the template branches on. Binary diffs are renderable:
    the viewer has its own state for them.
  */
  function hasRenderableDiff(diff: ParsedDiff | null): boolean {
    if (!diff) return false
    if (diff.empty_reason || diff.size_guard) return false
    const fileDiff = diff.file_diff
    return fileDiff.is_binary || fileDiff.hunks.some((h) => h.lines.length > 0)
  }

  /*
    The empty pane's heading and explanation, from core's reason rather than
    one caption covering three unrelated situations at once — which told the
    user the file was unchanged when the whitespace setting was simply hiding
    the change.
  */
  const EMPTY_DIFF_COPY: Record<EmptyDiffReason, { title: string; detail: string }> = {
    NoChanges: {
      title: 'No Changes',
      detail: 'This file matches its committed state.',
    },
    WhitespaceOnly: {
      title: 'Whitespace Only',
      detail: 'Every change here is whitespace, and Settings is set to hide those.',
    },
    NoTextualChanges: {
      title: 'No Textual Changes',
      detail:
        'The file changed without changing any lines — a mode change or rename, for example.',
    },
  }

  function emptyDiffCopy(diff: ParsedDiff | null): { title: string; detail: string } {
    return EMPTY_DIFF_COPY[diff?.empty_reason ?? 'NoTextualChanges']
  }

  /* What the size guard withheld, in the units the user thinks in. */
  function sizeGuardCopy(guard: DiffSizeGuard): string {
    return guard.reason === 'TotalBytes'
      ? `This diff is ${(guard.bytes / 1_048_576).toFixed(1)} MB — large enough to be slow to render.`
      : `This diff has a line of ${guard.longest_line} characters — long enough to be slow to render.`
  }

  // A submodule that is dirty inside but whose recorded commit hasn't moved
  // can't be staged from the parent repo (`git add` is a no-op), so it's never
  // eligible for commit selection. Every writer to `selectedFiles` skips these
  // so the user can't include one and hit "staging produced no changes".
  const isCommittable = (f: FileEntry) => !f.submodule_dirty

  // Consecutive *background* refresh failures. One is usually a transient index
  // lock mid-write and stays invisible; a streak means the repository is
  // genuinely unreadable (deleted, unmounted, permissions), which the user has
  // to be told about — otherwise the pane keeps rendering its last good
  // snapshot forever. Same threshold as the native client's
  // `quietFailureStreak`, and the same ownership: only the app's own timers
  // and resyncs move it. A refresh that follows a *user action* (commit,
  // discard, ignore, checkout) is silent for a different reason — the action
  // already reported its own outcome — and must not feed this, or three
  // index.lock races in a row would accuse a healthy repo of having vanished.
  let quietFailureStreak = 0
  const QUIET_FAILURE_THRESHOLD = 3

  /*
    The last status read, verbatim, as the equality gate compares it.

    Whole-value, not a hand-picked list of fields: the native client compares
    the whole `RepoStatus` through its synthesized `Equatable`, and a
    fingerprint naming fields by hand is a list someone has to remember to
    extend — the failure being that a *new* field stops moving the UI, silently,
    for whoever adds it. The string is the JSON the backend just sent, so
    building it costs one serialization of an object that was parsed a
    microsecond ago, and `stat_stamp` rides inside it: a file that was modified
    and still is compares equal without it, which is why an idle repository can
    be recognized at all.
  */
  let lastStatusJson: string | null = null

  /*
    How long, and over how many status reads, each excluded path has been
    missing from the file list.

    Kept beside `userDeselected` rather than inside it because every reader of
    that set asks one question — "is this path excluded?" — and a map of clocks
    would make each of them carry the answer to a different one. One writer
    (the reconcile below), rebuilt wholesale from core's answer, so the two
    cannot drift apart.
  */
  let exclusionClocks = new Map<string, Exclusion>()
  let lastReconcileAt = Date.now()
  /* `elapsed_ms` crosses as a u32, so it is clamped to that range at both ends:
     a machine resumed from a long sleep would otherwise hand over a number the
     backend can't take, and a clock that jumped backwards a negative one.
     Anything past the top is the same answer anyway — every grace window has
     expired — and anything below zero means no time has passed. */
  const MAX_ELAPSED_MS = 0xffff_ffff
  const NOTHING_PRUNED: ReadonlySet<string> = new Set()

  /**
   * Age the commit composer's opt-outs against the file list that just arrived
   * and answer with the paths whose grace window has run out.
   *
   * An opt-out used to be dropped the moment its path left the list, which
   * meant a formatter rewriting a file between two ticks silently re-included
   * it — and the next commit took a file the user had deliberately unchecked.
   * The rule is core's now ({@link exclusionsApi.reconcile}), shared with the
   * native client so the two cannot answer differently.
   *
   * The crossing is skipped entirely while nothing is excluded, which is the
   * usual state of the app: the poll pays for this only once the user has
   * actually unchecked something.
   */
  async function pruneExpiredExclusions(present: string[]): Promise<ReadonlySet<string>> {
    const now = Date.now()
    const elapsedMs = Math.min(Math.max(now - lastReconcileAt, 0), MAX_ELAPSED_MS)
    lastReconcileAt = now
    const excludedPaths = get(repoState).userDeselected
    if (excludedPaths.size === 0) {
      exclusionClocks.clear()
      return NOTHING_PRUNED
    }
    // A path with no clock yet is one the user has just unchecked: it starts at
    // zero on both terms, which is also what core answers for a present path.
    const excluded: Exclusion[] = [...excludedPaths].map(
      (path) => exclusionClocks.get(path) ?? { path, absent_ms: 0, absent_reads: 0 },
    )
    let kept: Exclusion[]
    try {
      kept = await exclusionsApi.reconcile(excluded, present, elapsedMs)
    } catch (error) {
      // A failed reconcile prunes nothing. Losing an opt-out costs a commit
      // nobody meant to make; keeping one costs a checkbox click.
      console.warn('[exclusions] reconcile failed, keeping every opt-out:', error)
      return NOTHING_PRUNED
    }
    exclusionClocks = new Map(kept.map((entry) => [entry.path, entry]))
    const survived = new Set(kept.map((entry) => entry.path))
    const pruned = new Set<string>()
    for (const entry of excluded) {
      if (!survived.has(entry.path)) pruned.add(entry.path)
    }
    return pruned
  }

  /**
   * Read the repository's status and publish it, returning what was read so
   * callers can act on the fields they care about without asking git a second
   * time — HEAD in particular, which used to cost its own `rev-parse` every
   * tick although `head_sha` was already in hand.
   */
  async function refreshStatus(
    opts: { silent?: boolean; background?: boolean } = {},
  ): Promise<GitStatus | null> {
    const repoPath = $appState.repoPath
    if (!repoPath) return null
    try {
      const status = await gitApi.getStatus(repoPath)
      // A read the user has moved on from is thrown away rather than published.
      // A switch resets the poll's memory and publishes the new repository
      // immediately, so a tick still in flight against the old one would
      // otherwise paint its branch, files and counts over the new repository —
      // and now also re-seed `lastHeadSha` and the fingerprint from it, which
      // is what makes this worth a guard rather than a shrug.
      if (get(appState).repoPath !== repoPath) return null
      // Clocks advance on every tick, changed list or not: a path is pruned for
      // having been *absent* long enough, which is precisely what an unchanged
      // file list keeps being true of.
      const pruned = await pruneExpiredExclusions(status.files.map((f) => f.path))
      if (get(appState).repoPath !== repoPath) return null
      const fingerprint = JSON.stringify(status)
      /*
        Nothing moved, so nothing is published. A repository nobody is editing
        produced a fresh `RepoState` — and three fresh `Set`s inside it — every
        two seconds, and every subscriber in the window re-rendered on it
        forever. Only the silent path takes the shortcut: an explicit refresh
        also clears the error modal on success, and that is a state change even
        when the status underneath is identical.
      */
      const unchanged =
        opts.silent === true &&
        fingerprint === lastStatusJson &&
        pruned.size === 0 &&
        get(repoState).statusLoaded
      lastStatusJson = fingerprint
      quietFailureStreak = 0
      if (unchanged) {
        // Any successful read still proves the repository is back, so a
        // standing poll banner retires even on a tick that publishes nothing.
        if (get(repoState).pollError !== undefined) {
          repoState.update((s) => ({ ...s, pollError: undefined }))
        }
        return status
      }
      repoState.update((s) => {
        const presentPaths = new Set(status.files.map((f) => f.path))
        const nextDeselected = new Set<string>()
        for (const p of s.userDeselected) {
          if (!pruned.has(p)) nextDeselected.add(p)
        }
        const nextSelected = new Set<string>()
        for (const f of status.files) {
          if (isCommittable(f) && !nextDeselected.has(f.path)) nextSelected.add(f.path)
        }
        // If the file the user was viewing has just been committed (or
        // otherwise dropped out of the working tree), drop the diff too —
        // otherwise the pane keeps rendering a diff for a path that no
        // longer exists in the changeset.
        const activeFileGone =
          s.activeFile !== null && !presentPaths.has(s.activeFile.path)
        return {
          ...s,
          statusLoaded: true,
          status: {
            branch: status.branch,
            upstream: status.upstream,
            hasUpstream: status.has_upstream,
            ahead: status.ahead,
            behind: status.behind,
            files: status.files,
            isMerging: status.merging,
            hasRemote: status.has_remote,
            unpushedShas: new Set(status.unpushed_shas ?? []),
            detached: status.detached,
            headSha: status.head_sha,
            proposal: status.proposal,
          },
          selectedFiles: nextSelected,
          userDeselected: nextDeselected,
          activeFile: activeFileGone ? null : s.activeFile,
          activeFileDiff: activeFileGone ? null : s.activeFileDiff,
          isDiffLoading: activeFileGone ? false : s.isDiffLoading,
          error: opts.silent ? s.error : undefined,
          // Any successful read proves the repository is back, so the banner
          // goes whoever asked for the read. Only the background path *sets*
          // it, so no user-facing error can be lost with it.
          pollError: undefined,
        }
      })
      // Keep the picker's badge for the active repo live off the same counts
      // the poll already computed — no extra fetch needed for the open repo.
      // `dirty` comes straight from the file list the Changes tab renders, so
      // for the visible repo dot and tab agree by construction.
      setRepoSync(repoPath, {
        ahead: status.ahead,
        behind: status.behind,
        hasRemote: status.has_remote,
        dirty: status.files.length > 0,
      })
      return status
    } catch (error) {
      if (!opts.silent) {
        // An explicit refresh (⌘R, a post-op reload) that failed — retrying it
        // is exactly what the user would do next.
        reportActionError(error, () => void refreshStatus(opts))
        return null
      }
      if (!opts.background) return null // a user action's own follow-up: it reported already
      // Swallow the blip, surface the streak. The banner is non-blocking on
      // purpose — a background tick must never seize the window the way an
      // action's failure modal does.
      quietFailureStreak += 1
      if (quietFailureStreak >= QUIET_FAILURE_THRESHOLD) {
        repoState.update((s) => ({ ...s, pollError: String(error) }))
      }
      return null
    }
  }

  async function refreshBranches(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const branches = await gitApi.listBranches(repoPath)
      repoState.update((s) => ({ ...s, branches }))
    } catch {}
  }

  async function loadInitialLog(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const commits = await gitApi.getLog(repoPath, PAGE_SIZE, 0)
      repoState.update((s) => ({
        ...s,
        log: {
          ...s.log,
          commits,
          hasMore: commits.length === PAGE_SIZE,
          loaded: true,
          resetSeq: s.log.resetSeq + 1,
        },
      }))
    } catch (error) {
      reportActionError(error)
    }
  }

  /*
    Re-read the log from HEAD, as deep as the user has paged and no deeper than
    MAX_COMMITS. Called when HEAD has moved — a commit, an undo, a checkout —
    which is the one thing that can invalidate rows the user is already looking
    at, and the reason the log is refetched rather than patched (FRONTEND §6.8).

    Re-reading from offset 0 is what keeps `commits[0] === HEAD` true for free.
    The old model refetched the *window* the user had slid to and then had to
    detect, after the fact, that its top was no longer HEAD — which it only
    checked past offset 0, so a new commit made while parked at the top of the
    list bumped nothing at all. There is no window to slide any more, so there
    is nothing to get wrong.

    `resetSeq` tells the list to go to row 0: it is the new HEAD, and an offset
    measured against a list whose top just changed means nothing.
  */
  async function refreshLog(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    const count = Math.min(Math.max(current.log.commits.length, PAGE_SIZE), MAX_COMMITS)
    try {
      const commits = await gitApi.getLog(repoPath, count, 0)
      repoState.update((s) => ({
        ...s,
        log: {
          ...s.log,
          commits,
          hasMore: commits.length === count,
          loaded: true,
          resetSeq: s.log.resetSeq + 1,
        },
      }))
    } catch {}
  }

  /**
   * React to HEAD having moved under us — a terminal commit, checkout or merge.
   *
   * Answered from the status the caller just read rather than from a
   * `rev-parse` of its own: porcelain v2 emits the HEAD OID as `# branch.oid`,
   * so `get_status` has already carried it at no cost, and the second
   * subprocess this used to spawn every tick was a duplicate of a field sitting
   * in the same reply. FRONTEND §6.1 mandates exactly that, and only the code
   * hadn't caught up.
   *
   * The branch list reloads with the history: a checkout in the terminal moves
   * the menu's checkmark and each branch's metadata, and reloading only the log
   * left the menu describing where HEAD used to be.
   */
  async function adoptHeadSha(headSha: string): Promise<void> {
    if (lastHeadSha === null) {
      lastHeadSha = headSha
      return
    }
    if (headSha === lastHeadSha) return
    lastHeadSha = headSha
    await Promise.all([refreshLog(), refreshBranches()])
  }

  /*
    Append the next page of older commits. Nothing is dropped from the front:
    the list only ever grows away from HEAD, so the row the user is looking at
    keeps its position and no scroll compensation is needed.

    Deduplicated by sha because the 2 s poll can re-read the log under a page
    already in flight; without it a commit could land in the list twice and
    give two rows the same key.
  */
  async function loadMoreCommits(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    const current = get(repoState)
    if (!current.log.hasMore || current.log.isPaging) return

    repoState.update((s) => ({ ...s, log: { ...s.log, isPaging: true } }))
    try {
      const fetched = await gitApi.getLog(repoPath, PAGE_SIZE, current.log.commits.length)
      repoState.update((s) => {
        const known = new Set(s.log.commits.map((c) => c.sha))
        return {
          ...s,
          log: {
            ...s.log,
            commits: [...s.log.commits, ...fetched.filter((c) => !known.has(c.sha))],
            hasMore: fetched.length === PAGE_SIZE,
            loaded: true,
            isPaging: false,
          },
        }
      })
    } catch (error) {
      repoState.update((s) => ({ ...s, log: { ...s.log, isPaging: false } }))
      // Reading further into the past is not an operation the user is blocked
      // on — the history they already have is still on screen and still
      // correct. Scrolling again re-asks, so the failure states itself and
      // gets out of the way instead of taking the window mid-scroll.
      reportNotice(error)
    }
  }

  async function loadDiffForFile(
    file: FileEntry | null,
    opts: { force?: boolean; showAnyway?: boolean } = {},
  ): Promise<void> {
    // Re-activating the same file that's already on screen (or already
    // being fetched) is a no-op — skip the refetch so arrow scrolls past
    // and back don't churn the pane. `force` overrides this so a focus-return
    // refresh re-fetches the open file even though its path is unchanged.
    const current = get(repoState)
    if (
      !opts.force &&
      file &&
      current.activeFile?.path === file.path &&
      (current.activeFileDiff !== null || current.isDiffLoading)
    ) {
      return
    }

    if (diffLoadingTimer) {
      clearTimeout(diffLoadingTimer)
      diffLoadingTimer = null
    }

    // Claim or release the size-guard reveal before the read, so the request
    // below can simply ask whether *this* file is the revealed one.
    if (opts.showAnyway) revealedDiffPath = file?.path ?? null
    else if (revealedDiffPath !== (file?.path ?? null)) revealedDiffPath = null

    // A submodule that is dirty inside but whose recorded commit hasn't moved
    // has no diff worth reading: git answers with an opaque
    // `Subproject commit …-dirty` line, which the pane replaces with an
    // explanation anyway. Branch here rather than in the template alone, or
    // every click on such a row spends a `git diff` on output nobody renders.
    const isDirtySubmodule = file?.submodule_dirty ?? false

    // Clearing to null on every switch caused the "Loading diff…" flash
    // even for sub-50 ms fetches. Now we keep the previous diff on screen
    // and only flip isDiffLoadingSlow=true after SLOW_DIFF_THRESHOLD_MS,
    // at which point the template dims it and overlays the spinner.
    repoState.update((s) => ({
      ...s,
      activeFile: file,
      isDiffLoading: file !== null && !isDirtySubmodule,
      isDiffLoadingSlow: false,
      // Drop the stale diff immediately when the user deselects, and when the
      // new selection is a row that will never produce one; only keep it on
      // screen during an actual transition between two rendered diffs.
      activeFileDiff: file === null || isDirtySubmodule ? null : s.activeFileDiff,
      activeFileDiffError: undefined,
    }))
    if (!file || isDirtySubmodule) return

    const repoPath = $appState.repoPath
    if (!repoPath) return

    diffLoadingTimer = setTimeout(() => {
      diffLoadingTimer = null
      const s = get(repoState)
      // Only escalate if the fetch we started is still the active one. The
      // payload deliberately stays: crossing the threshold dims what is on
      // screen, it does not unmount it. Dropping it here meant a slow load that
      // landed unchanged repainted and re-tokenized the whole pane from
      // scratch, and cost the user their scroll position on the way.
      if (s.activeFile?.path === file.path && s.isDiffLoading) {
        repoState.update((st) => ({ ...st, isDiffLoadingSlow: true }))
      }
    }, SLOW_DIFF_THRESHOLD_MS)

    try {
      const cfg = $config
      // One call: core reads and parses, and — when hide-whitespace left
      // nothing to show — checks the unfiltered diff so the pane can say the
      // change is there and the setting is hiding it.
      const parsed = await diffApi.getParsedDiff(
        repoPath,
        file,
        cfg?.hide_whitespace ?? false,
        webviewDiffOptions(cfg?.side_by_side_diff ?? false, revealedDiffPath === file.path),
      )
      // Drop the result if the user moved on to a different file mid-fetch.
      if (get(repoState).activeFile?.path !== file.path) return
      if (diffLoadingTimer) {
        clearTimeout(diffLoadingTimer)
        diffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        activeFileDiff: parsed,
        activeFileDiffError: undefined,
        isDiffLoading: false,
        isDiffLoadingSlow: false,
      }))
    } catch (error) {
      if (get(repoState).activeFile?.path !== file.path) return
      if (diffLoadingTimer) {
        clearTimeout(diffLoadingTimer)
        diffLoadingTimer = null
      }
      // Inline, in the pane that was going to show it, and with the stale diff
      // cleared: a failure to read *this* file is not an operation the window
      // has to stop for, and leaving the previous file's rows standing behind a
      // modal described one diff while rendering another (FRONTEND §6.3). The
      // retry is the row itself — the payload is gone, so clicking the file
      // again re-reads rather than short-circuiting on what is already open.
      repoState.update((s) => ({
        ...s,
        activeFileDiff: null,
        activeFileDiffError: String(error),
        isDiffLoading: false,
        isDiffLoadingSlow: false,
      }))
    }
  }

  /*
    What the reader asked to see past the size guard, per pane — the file for
    the changes pane, `sha|path` for the commit pane, since the same path in
    two commits is two different diffs.

    Kept as an identity rather than as a per-call flag, because the decision has
    to survive one thing and not the other: every re-read of the *same* diff
    keeps it — a layout change, a whitespace toggle, a poll that found the file
    rewritten — where re-arming the guard would silently take back what was
    asked for and, since the header lives inside the viewer, remove the control
    that got them past it. A different diff clears it, which is what makes the
    guard withhold rather than refuse.
  */
  let revealedDiffPath = $state<string | null>(null)
  let revealedCommitDiff = $state<string | null>(null)

  /**
   * Persist the diff layout the header just asked for.
   *
   * The choice outlives the file, the repository and the app, which is why it
   * is a config field and not component state — and why it is shared with the
   * native client rather than kept in `localStorage`. Publishing the config
   * moves `diffReadKey` below, which is what re-reads the open diffs with the
   * pairing the new arrangement needs.
   */
  function setDiffLayout(sideBySide: boolean): void {
    if (sideBySide === ($config?.side_by_side_diff ?? false)) return
    void patchConfig({ side_by_side_diff: sideBySide }).catch((e: unknown) => {
      // Nothing landed, so nothing changes on screen — the control renders
      // from the config and simply stays where it was.
      console.error('[config] could not save the diff layout', e)
    })
  }

  // Re-fetch the diff for the file currently open in the changes pane, because
  // its bytes on disk have moved. `force`, since the path is unchanged and the
  // ordinary path guard would treat the request as a no-op. No-op when nothing
  // is selected.
  function reloadActiveDiff(): void {
    const active = get(repoState).activeFile
    if (active) loadDiffForFile(active, { force: true })
  }

  // The size guard's escape: re-ask for the same diff with the guard lifted.
  // Deliberately not sticky — it applies to this one request, so moving to
  // another file gets the guard back rather than inheriting a decision made
  // about a different file.
  function showActiveDiffAnyway(): void {
    const active = get(repoState).activeFile
    if (active) loadDiffForFile(active, { force: true, showAnyway: true })
  }

  function showActiveCommitDiffAnyway(): void {
    const active = get(repoState).activeCommitFile
    if (active) loadCommitFileDiff(active, { force: true, showAnyway: true })
  }

  async function loadCommitFiles(commit: CommitInfo | null): Promise<void> {
    // Re-selecting the commit already open blanked the pane and refetched
    // everything it was showing, for a row the user clicked because it was
    // already the one they wanted. A commit's identity *is* its sha — its
    // files and totals cannot change without the sha changing — so there is
    // nothing a second read could return.
    if (commit && get(repoState).activeCommit?.sha === commit.sha) return
    repoState.update((s) => ({
      ...s,
      activeCommit: commit,
      activeCommitFiles: [],
      activeCommitStats: null,
      activeCommitFile: null,
      activeCommitFileDiff: null,
    }))
    if (!commit) return
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      // One call for both: the file list and the line totals come out of the
      // same `git log`, so they can't describe different commits and a stats
      // failure can't leave the header disagreeing with the list.
      const detail = await gitApi.getCommitDetail(repoPath, commit.sha)
      // Guard against a stale response if the user clicked another commit.
      if (get(repoState).activeCommit?.sha !== commit.sha) return
      repoState.update((s) => ({
        ...s,
        activeCommitFiles: detail.files,
        activeCommitStats: detail.stats,
      }))
      if (detail.files.length > 0) {
        loadCommitFileDiff(detail.files[0])
      }
    } catch {}
  }

  async function loadCommitFileDiff(
    file: FileEntry | null,
    opts: { force?: boolean; showAnyway?: boolean } = {},
  ): Promise<void> {
    const current = get(repoState)
    if (
      !opts.force &&
      file &&
      current.activeCommitFile?.path === file.path &&
      (current.activeCommitFileDiff !== null || current.isCommitDiffLoading)
    ) {
      return
    }

    if (commitDiffLoadingTimer) {
      clearTimeout(commitDiffLoadingTimer)
      commitDiffLoadingTimer = null
    }

    repoState.update((s) => ({
      ...s,
      activeCommitFile: file,
      isCommitDiffLoading: file !== null,
      isCommitDiffLoadingSlow: false,
      activeCommitFileDiff: file === null ? null : s.activeCommitFileDiff,
      activeCommitFileDiffError: undefined,
    }))
    if (!file) return

    const repoPath = $appState.repoPath
    const commit = get(repoState).activeCommit
    if (!repoPath || !commit) {
      repoState.update((s) => ({ ...s, isCommitDiffLoading: false, isCommitDiffLoadingSlow: false }))
      return
    }

    // Same rule as the changes pane, keyed on the commit as well: the same path
    // in two commits is two different diffs, so a reveal must not carry across.
    const revealKey = `${commit.sha}|${file.path}`
    if (opts.showAnyway) revealedCommitDiff = revealKey
    else if (revealedCommitDiff !== revealKey) revealedCommitDiff = null

    commitDiffLoadingTimer = setTimeout(() => {
      commitDiffLoadingTimer = null
      const s = get(repoState)
      // Same rule as the changes pane above: dim, don't unmount.
      if (s.activeCommitFile?.path === file.path && s.isCommitDiffLoading) {
        repoState.update((st) => ({ ...st, isCommitDiffLoadingSlow: true }))
      }
    }, SLOW_DIFF_THRESHOLD_MS)

    try {
      const parsed = await diffApi.getParsedCommitDiff(
        repoPath,
        commit.sha,
        file.path,
        webviewDiffOptions($config?.side_by_side_diff ?? false, revealedCommitDiff === revealKey),
      )
      if (get(repoState).activeCommitFile?.path !== file.path) return
      if (commitDiffLoadingTimer) {
        clearTimeout(commitDiffLoadingTimer)
        commitDiffLoadingTimer = null
      }
      repoState.update((s) => ({
        ...s,
        activeCommitFileDiff: parsed,
        activeCommitFileDiffError: undefined,
        isCommitDiffLoading: false,
        isCommitDiffLoadingSlow: false,
      }))
    } catch (error) {
      if (get(repoState).activeCommitFile?.path !== file.path) return
      if (commitDiffLoadingTimer) {
        clearTimeout(commitDiffLoadingTimer)
        commitDiffLoadingTimer = null
      }
      // Inline and cleared, exactly as the changes pane above.
      repoState.update((s) => ({
        ...s,
        activeCommitFileDiff: null,
        activeCommitFileDiffError: String(error),
        isCommitDiffLoading: false,
        isCommitDiffLoadingSlow: false,
      }))
    }
  }

  // Best-effort fetch of the active repo's remote. Swallows offline/auth/no-remote
  // errors so callers can always follow up with a status refresh regardless.
  // This is automatic (timer / refocus / cold-open), so it's gated on
  // connectivity: skipped while offline or backing off, and its outcome feeds
  // the breaker so a recovered link re-enables background syncing app-wide.
  async function fetchActiveRemote(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    if (!shouldAttemptBackground()) return
    // A repo with no remote has nothing to fetch: without this gate every
    // tick ran a doomed `git fetch`, two of them opened the breaker, and the
    // open breaker then suppressed *all* background fetches app-wide (tier
    // badges, refocus sweeps, the update check) for as long as that repo
    // stayed open. The tier path already gates on the same flag
    // (`repoSync.syncRepo`).
    //
    // Only skip when we *know* there is no remote. `hasRemote` defaults to
    // false, and a repo switch resets it, so between the switch and the first
    // status load an unqualified read would decide "no remote" about a repo
    // nobody has looked at — silently dropping a legitimate refocus fetch.
    const current = get(repoState)
    if (current.statusLoaded && !current.status.hasRemote) return
    let remote: string | null
    try {
      remote = await gitApi.getRemote(repoPath)
    } catch {
      return // local remote lookup failed — not a connectivity signal
    }
    // Now a real answer rather than an invented "origin", so this is the
    // authoritative version of the gate above rather than dead code under it.
    if (!remote) return
    try {
      // Automatic, so it runs on the background budget: an unreachable remote
      // gives up in 12 s instead of holding the single network slot for ten
      // minutes with every other repo's refresh queued behind it.
      await gitApi.fetch(repoPath, remote, true)
      recordResult(true)
    } catch {
      recordResult(false)
    }
  }

  async function performAutoFetch(): Promise<void> {
    await fetchActiveRemote()
    await refreshStatus({ silent: true, background: true })
  }

  /**
   * The active repository's status poll — how a change made outside the app,
   * in the terminal or an editor, appears by itself.
   *
   * A self-scheduling chain rather than a `setInterval`, because the cadence is
   * not a constant: it is the window's activity ladder (2 s frontmost, 10 s
   * visible, 30 s hidden), re-read after every tick. The loop also can't
   * overlap itself, which is what the in-flight flag used to be for — a cycle
   * that outran the interval on a repo under heavy load would otherwise stack a
   * second one on top of it, each spawning several git processes.
   */
  const statusLoop = pacedLoop({
    label: 'status-poll',
    dueAt: (lastRunAt) => lastRunAt + statusPollIntervalMs(),
    run: async () => {
      if (get(appState).phase !== 'main') return
      // Paused while a push/pull/publish runs: polling mid-transfer only adds
      // git processes that contend with it for the repo's disk and locks. The
      // op's own handler refreshes status when it completes.
      if (!canPollStatus()) return
      const status = await refreshStatus({ silent: true, background: true })
      if (status) await adoptHeadSha(status.head_sha)
    },
  })

  /**
   * How often the automatic fetch should run under this config, or 0 for never
   * — the one place the two settings are read together, so the launch arm, a
   * repo switch and a settings change can't disagree about what "auto-fetch
   * off" means.
   */
  function configuredFetchIntervalMs(cfg: Config | null): number {
    return cfg?.auto_fetch ? cfg.fetch_interval_ms || 30000 : 0
  }

  /**
   * The active repository's automatic fetch.
   *
   * Switched off, the loop parks rather than running an idle re-check: turning
   * it back on re-arms it directly through the config effect below, including
   * when the change was made in the native client (which arrives on the next
   * wake-up, with the config re-read).
   *
   * Held back while text has the keyboard, asked at the moment of the tick: a
   * fetch can reorder the file list, and doing that under a half-written commit
   * message or a live checkbox is the one background effect the user would
   * actually feel. The terminal counts as text entry, deliberately and in both
   * clients — a shell is exactly where the list is being changed from.
   */
  const fetchLoop = pacedLoop({
    label: 'auto-fetch',
    skewFirstMs: SESSION_FETCH_SKEW_MS,
    dueAt: (lastRunAt) => {
      const configured = configuredFetchIntervalMs(get(config))
      return configured > 0
        ? lastRunAt + pacedFetchIntervalMs(configured)
        : Number.POSITIVE_INFINITY
    },
    run: async () => {
      if (get(appState).phase !== 'main') return
      if (!canAutoFetch() || isTextInputFocused()) return
      await performAutoFetch()
    },
  })

  // Re-sync when the window wakes up — regains focus, or comes back on screen.
  // Status and HEAD may have moved while we were away.
  //
  // The open file's diff is deliberately *not* re-fetched here any more. It used
  // to be, unconditionally, because there was no way to tell whether the file
  // had changed; the stamp effect below is that way, and it fires off the very
  // status read this function awaits. Keeping both meant two `git diff`s of the
  // same file racing on every single activation, with nothing deciding which
  // answer won — and a read on every activation where nothing had changed at all.
  //
  // Guarded against overlapping runs: waking from hidden straight to frontmost
  // is two steps up the ladder, and both of them are a wake-up.
  let resyncing = false
  async function resyncOnActive(): Promise<void> {
    // Skipped during a network op for the same reason the poll pauses; the
    // op's completion refresh covers whatever this resync would have found.
    if (resyncing || $activeNetworkOp || $appState.phase !== 'main') return
    resyncing = true
    try {
      // The config file is shared with the native client and editable outside
      // this process, so re-read it before anything that consumes it. Without
      // this a save made elsewhere — theme, diff settings, provider — never
      // reached a running window at all: the staleness window was the lifetime
      // of the app. Re-read first so the refreshes below already see it.
      await refreshConfig()
      // Coming back to the app: fetch the active repo so a remote that moved
      // while we were away surfaces on the Pull button, then refresh local state.
      await fetchActiveRemote()
      const status = await refreshStatus({ silent: true, background: true })
      if (status) void adoptHeadSha(status.head_sha)
      // Also refresh the top recents tier so their picker badges aren't stale;
      // the tier loop itself was parked while we were away.
      repoSyncScheduler.kickTopTier()
    } finally {
      resyncing = false
    }
  }

  /**
   * The window changed activity state: re-decide every cadence that depends on
   * it, and catch up if it woke up.
   */
  function handleActivityChange(state: ActivityState, previous: ActivityState): void {
    statusLoop.reschedule()
    fetchLoop.reschedule()
    repoSyncScheduler.onActivityChange()
    if (wokeUp(state, previous)) void resyncOnActive()
  }

  function handleFileActivate(file: FileEntry) {
    loadDiffForFile(file)
  }

  /*
    Keep a file open in the diff pane: the first one when the changeset arrives,
    and again when a reload drops the one that was open. Landing on "Select a
    file to view its diff" with the list of files right there made the most
    common next action a click nobody should have to make — and this client's
    own commit-detail pane has always auto-selected its first file.

    Keyed on the *path list* rather than the files, so a tick that only changes
    a file's content or status can't disturb the selection: a derived string
    settles before the effect sees it, which is what turns a 2 s poll into a
    trigger that fires when the changeset actually changes. `refreshStatus` has
    already cleared `activeFile` when its path left the tree, so "nothing open"
    is the whole condition — the same two cases the native sidebar re-seats on.
  */
  const changedPaths = $derived($repoState.status.files.map((f) => f.path).join('\n'))
  $effect(() => {
    const paths = changedPaths
    untrack(() => {
      if (!paths) return
      const current = get(repoState)
      if (current.activeFile) return
      const first = current.status.files[0]
      if (first) void loadDiffForFile(first)
    })
  })

  /*
    Keep the open diff in step with the file on disk.

    The pane used to go stale until the row was reselected: a poll tick brought
    the file's new bytes into the status reply and nothing looked at them. What
    makes the edit visible is `stat_stamp` — core's mtime + size for the
    working-tree side of each entry — because porcelain v2 carries no worktree
    hash, so a file that was modified and is still modified reads identically
    from one tick to the next. `xy` rides in the key too, so staging or
    unstaging the file reloads it as well: the bytes didn't move, but the diff
    being shown is against a different side.

    `head_sha` rides in the key too, for the half `stat_stamp` cannot see: the
    diff is HEAD against the working tree, so moving HEAD changes it even when
    the file is untouched. A `--mixed` reset is the case that proves it — the
    bytes on disk and the status letters both stay exactly as they were while
    the diff grows by everything the reset commit contained.

    Keyed per file, so an unrelated edit elsewhere in the tree re-tokenizes
    nothing here. Same shape as the auto-select rule above: a `$derived` string
    settles before the effect sees it, and the effect reloads only when the
    *same* path's key moved. A different path means the user changed the
    selection, and that selection's own load is already in flight. NUL joins the
    key's parts because it is the one byte a git path cannot contain, so "is
    this the same file?" can't be fooled by a filename with a space in it.
  */
  const activeFileStamp = $derived.by(() => {
    const active = $repoState.activeFile
    if (!active) return ''
    const entry = $repoState.status.files.find((f) => f.path === active.path)
    if (!entry) return ''
    return [
      entry.path,
      entry.xy,
      entry.stat_stamp ?? '',
      $repoState.status.headSha,
    ].join('\u0000')
  })
  let lastFileStamp = ''
  $effect(() => {
    const stamp = activeFileStamp
    untrack(() => {
      const previous = lastFileStamp
      lastFileStamp = stamp
      if (!stamp || !previous) return
      const path = stamp.slice(0, stamp.indexOf('\u0000'))
      if (!previous.startsWith(`${path}\u0000`)) return
      reloadActiveDiff()
    })
  })

  /*
    The History tab's half of the same rule, and native's exact two conditions:
    keep the newest commit selected on arrival, and re-seat when a refresh drops
    the selected sha — which is what an amend or an undo does to it. Landing on
    "Select a commit" beside a list of commits is the same wasted click the
    changes pane used to ask for, and rendering the detail of a commit that has
    been rewritten away is worse than empty: it is wrong.

    Keyed on the *sha list*, and only while History is the visible tab — the
    pane stays mounted behind Changes, and selecting into it there would spend a
    `git log` and a diff read on a pane nobody is looking at. The leading `#`
    keeps "History is showing an empty log" (clear the selection) distinct from
    "History isn't showing" (do nothing), which a bare empty string cannot.
  */
  const historySelectionKey = $derived(
    $repoState.activeTab === 'history' && $repoState.log.loaded
      ? `#${$repoState.log.commits.map((c) => c.sha).join('\n')}`
      : '',
  )
  $effect(() => {
    const key = historySelectionKey
    untrack(() => {
      if (!key) return
      const current = get(repoState)
      const selected = current.activeCommit?.sha
      if (selected && current.log.commits.some((c) => c.sha === selected)) return
      void loadCommitFiles(current.log.commits[0] ?? null)
    })
  })

  /**
   * Reload everything an operation against the remote can have changed: status
   * (branch, counts, the file list) and the log together.
   *
   * Status alone was the old shape, and it left History up to two seconds
   * behind a pull that had already brought commits in. Native reloads both
   * after every network operation; this is the same rule, and it also covers
   * the local operations that move HEAD.
   *
   * `lastHeadSha` is re-seeded from the status this already read, so the next
   * poll tick doesn't see a moved HEAD and refetch the log a second time.
   *
   * `silent` belongs to the caller: an operation that already reported its own
   * outcome (a commit) passes it, while one the user is still waiting to see
   * finish (a transfer, ⌘R) doesn't, so a failed read reaches them.
   */
  async function reloadAfterHeadMove(opts: { silent?: boolean } = {}): Promise<void> {
    const [status] = await Promise.all([refreshStatus(opts), refreshLog()])
    if (status) lastHeadSha = status.head_sha
  }

  /**
   * ⌘R — the forced reload, and the only route to one now that the sync ladder
   * replaced the header's Refresh button. Status, history and the branch list
   * together, which is what the native client's View ▸ Refresh already did;
   * this client used to reload status alone.
   *
   * Held back while a transfer runs, for the reason the 2 s poll pauses: a
   * `git status` racing a pull contends for the very lock files it is writing.
   */
  async function forceReload(): Promise<void> {
    if (!$appState.repoPath || $activeNetworkOp) return
    await Promise.all([reloadAfterHeadMove(), refreshBranches()])
  }

  async function handleCommitted(): Promise<void> {
    // Defensive: clear amend mode if the composer somehow didn't.
    repoState.update((s) => ({ ...s, commitToAmend: null }))
    await reloadAfterHeadMove({ silent: true })
  }

  function handleStartAmending(commit: CommitInfo): void {
    repoState.update((s) => ({ ...s, commitToAmend: commit, activeTab: 'changes' }))
  }

  function handleStopAmending(): void {
    repoState.update((s) => ({ ...s, commitToAmend: null }))
  }

  // ---- Check Out Commit (detached HEAD) ------------------------------------
  // Commit pending a checkout confirmation; null when the dialog is closed.
  let checkoutTarget = $state<CommitInfo | null>(null)
  let isCheckingOut = $state(false)

  function handleCheckoutCommit(commit: CommitInfo): void {
    checkoutTarget = commit
  }

  async function confirmCheckout(): Promise<void> {
    const repoPath = $appState.repoPath
    const commit = checkoutTarget
    if (!repoPath || !commit) return
    isCheckingOut = true
    try {
      await gitApi.checkoutCommit(repoPath, commit.sha)
      // The checked-out commit is now HEAD. Seed lastHeadSha so the poll doesn't
      // redundantly reload the log, then refresh status (detached state) and the
      // log (now rooted at this commit), mirroring handleCommitted.
      lastHeadSha = commit.sha
      // Branches too: HEAD is now detached, so the picker's current-branch
      // marker and every branch's ahead/behind are stale.
      await Promise.all([
        refreshStatus({ silent: true }),
        refreshLog(),
        refreshBranches(),
      ])
    } catch (error) {
      reportActionError(error)
    } finally {
      // Always close the dialog so any error surfaces in the ErrorModal alone.
      checkoutTarget = null
      isCheckingOut = false
    }
  }

  function cancelCheckout(): void {
    if (!isCheckingOut) checkoutTarget = null
  }

  async function handleUndoCommit(commit: CommitInfo): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await gitApi.undoLastCommit(repoPath)
      // Set the seed BEFORE refresh so the composer prefills as soon as the
      // tab switches over. The backend pre-parses the co-authors / stripped
      // body off the commit's trailers. Also defensively clear amend mode in
      // case the undone commit happened to be the one the user was amending.
      repoState.update((s) => ({
        ...s,
        commitToAmend: null,
        restoreMessage: {
          summary: commit.summary,
          description: commit.body_without_coauthors,
          coAuthors: commit.co_authors,
        },
        activeTab: 'changes',
      }))
      await handleCommitted()
      // The branch just lost a commit — its ahead count and the row's
      // metadata in the picker moved with it.
      await refreshBranches()
    } catch (error) {
      reportActionError(error)
    }
  }

  // Master select-all from the FileList header. `selectAll === true` mirrors
  // the user explicitly opting every file in (clearing userDeselected); false
  // mirrors opting every file out (so refreshStatus won't re-add them next
  // poll tick).
  function handleToggleAll(selectAll: boolean) {
    repoState.update((s) => {
      if (selectAll) {
        return {
          ...s,
          selectedFiles: new Set(s.status.files.filter(isCommittable).map((f) => f.path)),
          userDeselected: new Set(),
        }
      }
      return {
        ...s,
        selectedFiles: new Set(),
        userDeselected: new Set(s.status.files.map((f) => f.path)),
      }
    })
  }

  // Range toggle from FileList — shift+click on a checkbox or Space on a
  // multi-row selection. Same opt-out tracking as handleFileToggle so the 2s
  // refreshStatus poll doesn't re-include paths the user just deselected.
  function handleBulkToggle(paths: string[], include: boolean) {
    repoState.update((s) => {
      const nextSelected = new Set(s.selectedFiles)
      const nextDeselected = new Set(s.userDeselected)
      // A range can sweep over a dirty submodule; it can never be included.
      const blocked = new Set(
        s.status.files.filter((f) => !isCommittable(f)).map((f) => f.path),
      )
      for (const p of paths) {
        if (include && !blocked.has(p)) {
          nextSelected.add(p)
          nextDeselected.delete(p)
        } else if (!include) {
          nextSelected.delete(p)
          nextDeselected.add(p)
        }
      }
      return { ...s, selectedFiles: nextSelected, userDeselected: nextDeselected }
    })
  }

  function handleFileToggle(file: FileEntry) {
    // The row's checkbox is disabled for dirty submodules; ignore any toggle
    // that still reaches here (e.g. a programmatic call) so it stays excluded.
    if (!isCommittable(file)) return
    repoState.update((s) => {
      const nextSelected = new Set(s.selectedFiles)
      const nextDeselected = new Set(s.userDeselected)
      if (nextSelected.has(file.path)) {
        nextSelected.delete(file.path)
        nextDeselected.add(file.path)
      } else {
        nextSelected.add(file.path)
        nextDeselected.delete(file.path)
      }
      return { ...s, selectedFiles: nextSelected, userDeselected: nextDeselected }
    })
  }

  // ---- Changes-tab context menu --------------------------------------------
  // Files pending a discard confirmation; null when the dialog is closed.
  let discardTarget = $state<FileEntry[] | null>(null)
  // What discarding them would actually do, as core decides it — the same
  // decision the discard itself runs on, so the dialog can't promise something
  // the action then doesn't do. Null until the answer arrives.
  let discardPlan = $state<DiscardPlan | null>(null)
  let isDiscarding = $state(false)
  // Why the last attempt was refused, kept in the dialog that raised it rather
  // than sent to the action modal — see `DiscardConfirm`'s `error` prop.
  let discardError = $state<string | undefined>(undefined)

  // Run a side-effect-only file action (copy / reveal / open). These hand the
  // file to another program and change nothing here, so a failure is reported
  // and stepped over: taking the window because Finder wouldn't open is a
  // bigger interruption than the thing that failed, and the repository the user
  // was actually looking at stays on screen behind the banner. No-op without an
  // open repo.
  function runFileAction(fn: (repoPath: string) => Promise<void>): void {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    fn(repoPath).catch(reportNotice)
  }

  async function ignoreFiles(append: (repoPath: string) => Promise<void>): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      await append(repoPath)
      // The newly-ignored untracked file drops out of the changes list.
      await refreshStatus({ silent: true })
    } catch (error) {
      // Retryable: an append to `.gitignore` that lost an `index.lock` race
      // wants exactly the same call again, and the closure still holds which
      // file or extension it was for.
      reportActionError(error, () => void ignoreFiles(append))
    }
  }

  function requestDiscard(files: FileEntry[]): void {
    if (files.length === 0) return
    discardTarget = files
    discardPlan = null
    discardError = undefined
    void classifyDiscard(files)
  }

  /** Ask core what discarding `files` would do, ignoring the answer if the
   *  dialog it was for has since been dismissed or re-aimed. */
  async function classifyDiscard(files: FileEntry[]): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath) return
    try {
      const plan = await gitApi.classifyDiscard(repoPath, files)
      if (discardTarget === files) discardPlan = plan
    } catch {
      // No outcome line beats a wrong one; the dialog says it is still working
      // it out, and the discard itself runs on core's decision either way.
    }
  }

  async function confirmDiscard(): Promise<void> {
    const repoPath = $appState.repoPath
    const files = discardTarget
    if (!repoPath || !files) return
    isDiscarding = true
    discardError = undefined
    try {
      await gitApi.discardFiles(repoPath, files)
      discardTarget = null
      discardPlan = null
      // refreshStatus prunes the discarded files from the list / active diff.
      await refreshStatus({ silent: true })
    } catch (error) {
      discardError = String(error)
      // A refusal is not proof that nothing happened: core restores from HEAD
      // and trashes in separate steps, so the first can land and the second
      // fail. Re-read the tree either way, and re-ask what a retry would now
      // do — an outcome line describing a tree that no longer exists is worse
      // than none.
      await refreshStatus({ silent: true })
      await classifyDiscard(files)
    } finally {
      isDiscarding = false
    }
  }

  function cancelDiscard(): void {
    if (isDiscarding) return
    discardTarget = null
    discardPlan = null
    discardError = undefined
  }

  // Intents raised by FileList's right-click menu. Repo path + refresh + the
  // confirm dialog live here; FileList only decides what to show.
  const fileContextActions: FileContextActions = {
    discard: requestDiscard,
    ignoreFile: (file) => void ignoreFiles((repo) => fileActions.ignoreFile(repo, file)),
    ignoreExtension: (ext) => void ignoreFiles((repo) => fileActions.ignoreExtension(repo, ext)),
    copyPath: (file) => runFileAction((repo) => fileActions.copyAbsolutePath(repo, file)),
    copyRelativePath: (file) => runFileAction(() => fileActions.copyRelativePath(file)),
    reveal: (file) => runFileAction((repo) => fileActions.revealFile(repo, file)),
    openWithDefault: (file) => runFileAction((repo) => fileActions.openWithDefault(repo, file)),
  }

  /**
   * Forget everything the background loops remember *about one repository*.
   *
   * Each of these answers a question of the form "has this changed since I last
   * looked?", and carrying an answer about the old repository into the new one
   * is wrong in a different way each time: a stale HEAD refetches the log (or
   * fails to), a stale status fingerprint suppresses the first publish, a stale
   * file stamp reloads a diff that was never open, a stale exclusion clock prunes
   * an opt-out the user has not made yet, and a carried failure streak lets two
   * failures on the old repository plus one on the new raise the new one's
   * banner. `resetRepoState` clears `pollError` itself.
   */
  function resetPollState(): void {
    lastHeadSha = null
    lastStatusJson = null
    lastFileStamp = ''
    exclusionClocks.clear()
    lastReconcileAt = Date.now()
    quietFailureStreak = 0
  }

  async function handleSwitchRepo(repo: string) {
    if (!repo || repo === $appState.repoPath) {
      showRepos = false
      return
    }
    showRepos = false
    resetPollState()
    resetRepoState()
    appState.update((s) => ({ ...s, repoPath: repo }))
    await patchReposState({ last_opened_repo: repo })
    // Promote to the front of the recents list (re-tiers the background sync)
    // and fetch it now — "open a repo" should always pull its latest counts,
    // including for the untiered long tail.
    recordRecentRepo(repo)
    repoSyncScheduler.syncOnSwitch(repo)
    try {
      const [status] = await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
      if (status) lastHeadSha = status.head_sha
      // Re-start rather than reschedule: the new repository's cadence is
      // measured from the fetch that just opened it, not from the last one the
      // previous repository ran.
      fetchLoop.start()
    } catch (error) {
      reportActionError(error)
    }
  }

  // A `leogit <dir>` invocation reached the already-running app (forwarded by
  // the single-instance plugin as an `open-repo` event), or a folder was just
  // initialised from App's prompt. Make the repo selectable — it may live
  // outside the scan paths — then switch to it. Re-running `leogit .` on the
  // open repo is a no-op beyond the window focus the backend already did.
  //
  // Exported so App can hand off a freshly created repo: only this component
  // can reset the open repo's view state, and it is already mounted.
  export async function openExternalRepo(path: string) {
    if (!path) return
    console.log('[launch] open-repo event — switching to:', path)
    if (!$appState.repos.includes(path)) {
      appState.update((s) => ({ ...s, repos: [...s.repos, path] }))
    }
    await handleSwitchRepo(path)
  }

  // Open the Clone dialog from the repo dropdown, seeding its destination from
  // the shared rule (last-used clone folder → first scan path → ~/Dev).
  async function openClone() {
    showRepos = false
    cloneDefaultDir = await resolveCloneDefaultDir()
    showClone = true
  }

  // A clone finished: remember where it landed, make it selectable, and open it.
  async function handleCloned(repoPath: string, parentDir: string) {
    showClone = false
    await rememberCloneDir(parentDir)
    appState.update((s) => ({
      ...s,
      repos: s.repos.includes(repoPath) ? s.repos : [...s.repos, repoPath],
    }))
    await handleSwitchRepo(repoPath)
  }

  /**
   * Show the repo dropdown, re-walking first so a repo cloned from a terminal
   * or a folder just added to the scan paths is in the list the user is about
   * to read. Not awaited: the list on screen is already usable, and the walk
   * publishes into it when it lands.
   */
  function openRepos() {
    showRepos = true
    void rediscoverRepos()
  }

  // ---- Branches & merge ----------------------------------------------------

  /**
   * The branch operation in flight, or null.
   *
   * One at a time. Two checkouts issued by a double-click contend on
   * `index.lock`, and until now nothing here said a slow one was still running.
   * Every handler below refuses to start a second, and **a refusal is never
   * reported as a success**: each returns without dismissing the surface that
   * asked, so no dialog closes as though the work had been done.
   */
  type BranchOp = 'switch' | 'create' | 'delete' | 'merge' | 'abort'
  let branchOp = $state<BranchOp | null>(null)

  /** Source branch of the pending merge dialog; null when it is closed. */
  let mergeSource = $state<string | null>(null)
  /** How many commits that merge would bring in — null until the count lands. */
  let mergeCommitCount = $state<number | null>(null)
  /** Branch pending a delete confirmation; null when it is closed. */
  let deleteTarget = $state<string | null>(null)
  let showAbortMerge = $state(false)

  /**
   * Open the branch popover, re-reading the list on the way.
   *
   * The list used to be reloaded at five call sites, none of them this one, so
   * a branch created in the embedded terminal could stay invisible for the
   * whole session — the status poll only notices the ones that move HEAD. One
   * `for-each-ref` at the moment of intent is what the native menu does.
   */
  function openBranches(): void {
    showBranches = true
    void refreshBranches()
  }

  /**
   * Post-op reload for anything the branch menu does that moves HEAD — a
   * switch, a create-and-switch, a merge, an abort. Status, history and the
   * branch list together.
   *
   * These handlers used to call `refreshStatus()` and then `handleCommitted()`,
   * which reads status a second time and throws the first read away.
   */
  async function reloadAfterBranchChange(): Promise<void> {
    // Amend targets HEAD, and HEAD just moved — quite possibly onto a branch
    // where the commit being amended doesn't exist at all.
    repoState.update((s) => ({ ...s, commitToAmend: null }))
    await Promise.all([reloadAfterHeadMove({ silent: true }), refreshBranches()])
  }

  async function handleSwitchBranch(branch: string): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath || branchOp) return
    // Checking out the branch you are already on spends a checkout and a full
    // refresh chain to arrive exactly where you started.
    if (branch === $repoState.status.branch) return
    branchOp = 'switch'
    try {
      await gitApi.switchBranch(repoPath, branch)
      showBranches = false
      await reloadAfterBranchChange()
    } catch (error) {
      // A branch change is FRONTEND §6.13's own example of a failure the user
      // is waiting on: the popover closes and the modal says why it didn't
      // happen, with the same attempt one click away.
      showBranches = false
      reportActionError(error, () => void handleSwitchBranch(branch))
    } finally {
      branchOp = null
    }
  }

  /**
   * Create a branch and land on it. Resolves to the failure text rather than
   * raising a modal: the dropdown's form keeps the typed name and states the
   * refusal under the field, because the field is where the fix is
   * (FRONTEND §6.13). It used to clear the name *before* the outcome and put
   * the error in a modal over a closed dropdown — so a rejected name had to be
   * retyped from memory.
   */
  async function handleCreateBranch(name: string): Promise<string | undefined> {
    const repoPath = $appState.repoPath
    if (!repoPath) return 'No repository is open.'
    if (branchOp) return 'Another branch operation is still running.'
    branchOp = 'create'
    try {
      await gitApi.createBranch(repoPath, name, '')
      await gitApi.switchBranch(repoPath, name)
      showBranches = false
      await reloadAfterBranchChange()
      return undefined
    } catch (error) {
      return String(error)
    } finally {
      branchOp = null
    }
  }

  function requestDeleteBranch(name: string): void {
    showBranches = false
    deleteTarget = name
  }

  async function deleteBranch(name: string): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath || branchOp) return
    branchOp = 'delete'
    try {
      await gitApi.deleteBranch(repoPath, name)
      // HEAD cannot move: git refuses to delete the branch you are on, and the
      // dropdown never offers it. Only the list changed.
      await refreshBranches()
      deleteTarget = null
    } catch (error) {
      deleteTarget = null
      reportActionError(error, () => void deleteBranch(name))
    } finally {
      branchOp = null
    }
  }

  /**
   * Open the merge dialog for `source` → the checked-out branch, and start
   * reading how many commits it would bring in.
   *
   * `count_commits_to_merge` counts what its argument holds that HEAD does not
   * — the commits this merge brings in. Its parameter is named `targetBranch`,
   * which reads backwards here: the *source* is what it wants.
   */
  function requestMerge(source: string): void {
    showBranches = false
    mergeSource = source
    mergeCommitCount = null
    const repoPath = $appState.repoPath
    if (!repoPath) return
    gitApi
      .countCommitsToMerge(repoPath, source)
      .then((count) => {
        // Ignore an answer about a dialog the user already dismissed.
        if (mergeSource === source) mergeCommitCount = count
      })
      .catch(() => {})
  }

  /**
   * What to say about a merge git refused. `error_message` carries git's own
   * output, which already names the conflicted paths; the conflict list is the
   * fallback for a refusal that printed nothing useful.
   */
  function mergeFailureText(result: MergeResult, source: string): string {
    if (result.error_message) return result.error_message
    if (result.conflicts.length > 0) {
      const n = result.conflicts.length
      return `Merging ${source} left ${n} conflicted ${n === 1 ? 'file' : 'files'}:\n${result.conflicts.join('\n')}`
    }
    return `Could not merge ${source}.`
  }

  /**
   * Run the merge the dialog is showing. `squash` stages the result instead of
   * committing it, so it takes a second call with git's generated message —
   * the same two-step the native client runs.
   *
   * A refusal closes the dialog and takes the modal rather than staying inline:
   * unlike a rejected publish name, a conflicted merge is not fixed by pressing
   * the button again. It has already changed the repository, and the work
   * continues in the Changes tab, where the conflicted files are waiting.
   */
  async function runMerge(squash: boolean): Promise<void> {
    const repoPath = $appState.repoPath
    const source = mergeSource
    if (!repoPath || !source || branchOp) return
    branchOp = 'merge'
    try {
      const result = squash
        ? await gitApi.mergeSquash(repoPath, source)
        : await gitApi.mergeBranch(repoPath, source)
      if (result.success && squash) await gitApi.commitSquashMerge(repoPath)
      mergeSource = null
      // Either outcome moved the working tree — a clean merge advanced HEAD, a
      // conflicted one left MERGE_HEAD and conflicted files behind — so the
      // reload happens before the failure is reported.
      await reloadAfterBranchChange()
      if (!result.success) reportActionError(mergeFailureText(result, source))
    } catch (error) {
      mergeSource = null
      await reloadAfterBranchChange()
      reportActionError(error)
    } finally {
      branchOp = null
    }
  }

  function requestAbortMerge(): void {
    showBranches = false
    showAbortMerge = true
  }

  /**
   * Abort the merge in progress. Until now this client had no route to one at
   * all: a merge started in the embedded terminal showed `MERGING` in the
   * header with no way out of it that wasn't the terminal again.
   */
  async function abortMerge(): Promise<void> {
    const repoPath = $appState.repoPath
    if (!repoPath || branchOp) return
    branchOp = 'abort'
    try {
      await gitApi.mergeAbort(repoPath)
      showAbortMerge = false
      await reloadAfterBranchChange()
    } catch (error) {
      showAbortMerge = false
      // A failed abort may still have unwound part of the merge, so reload
      // before saying so.
      await reloadAfterBranchChange()
      reportActionError(error, () => void abortMerge())
    } finally {
      branchOp = null
    }
  }

  // Retry closes the modal first: the second attempt reports its own outcome,
  // and leaving the first failure on screen while it runs would make a success
  // look like nothing happened.
  function retryError(): void {
    const retry = get(repoState).errorRetry
    dismissActionError()
    retry?.()
  }

  function toggleTerminalMinimize() {
    if (!terminalExpanded && terminalSessionId === 0) {
      terminalSessionId = 1
    }
    terminalExpanded = !terminalExpanded
  }

  /**
   * Run `command` in the embedded terminal, starting and expanding it first if
   * it isn't already up.
   *
   * This is why the app hands a fix off to its own terminal rather than running
   * it itself: `claude auth login` opens a browser and then waits on stdin for
   * a pasted code. Driving that from our own UI would mean rebuilding a
   * terminal — and asking for an auth code in app chrome, which is a habit
   * worth not teaching. Here the real CLI runs in a real shell, the browser
   * does the sign-in, and the user answers the tool's own prompt.
   */
  async function runInTerminal(command: string): Promise<void> {
    if (terminalSessionId === 0) terminalSessionId = 1
    terminalExpanded = true
    // The panel may have been created by the click that got us here, so wait
    // for it to mount before reaching for it; `runCommand` queues from there
    // until the shell itself is ready.
    await tick()
    terminal?.runCommand(command)
  }

  function newTerminalSession() {
    // Clear the old label so the header can't show the previous session's
    // shell while the new one is still starting.
    activeShellLabel = ''
    terminalSessionId += 1
    terminalExpanded = true
  }

  function killTerminalSession() {
    activeShellLabel = ''
    terminalSessionId = 0
    terminalExpanded = false
  }

  function handleKeyDown(e: KeyboardEvent) {
    const inField = isTextInputElement(e.target)
    const meta = e.ctrlKey || e.metaKey

    // The terminal owns its keys. The panel's own toggle is the single
    // exception — xterm hands it back to us via `attachCustomKeyEventHandler`,
    // and it has to be handled *before* the `inField` bail below, because
    // xterm's input sink is a <textarea>. See `utils/keyboard.ts`.
    //
    // The toggle is `Ctrl` on every platform, as in the native client: ⌘`
    // belongs to macOS's window cycling. This test and the one
    // xterm's own handler makes have to keep agreeing, or the chord stops
    // working from inside the panel — the one place it is most wanted.
    if (isFromTerminal(e)) {
      if (e.ctrlKey && e.key === '`') {
        e.preventDefault()
        toggleTerminalMinimize()
      }
      return
    }

    // Escape belongs to whatever is frontmost, and each surface registers
    // itself while it is on screen — so there is no list here to keep in step
    // with the one in `App.svelte`, and dismissing a confirmation no longer
    // takes the popover that raised it with it.
    if (e.key === 'Escape' && dismissTopOverlay()) {
      e.preventDefault()
      return
    }

    // The panel toggle, also above the `inField` bail. `Ctrl` + `` ` `` is not
    // something anyone types into a commit message, so a field holding focus is
    // no reason to refuse it — and the terminal is exactly where you go *from*
    // the composer, to run the thing you are about to describe. The native
    // client binds it as a key equivalent, which fires ahead of the first
    // responder and so was never gated on focus at all. This is the second of
    // the two tests that own the chord (the first is a few lines up, for
    // events raised inside the panel).
    if (e.ctrlKey && e.key === '`') {
      e.preventDefault()
      toggleTerminalMinimize()
      return
    }

    // The composer's own chords, deliberately above the `inField` bail: they
    // are *for* the fields, and a shortcut you have to leave the message to use
    // is one nobody reaches mid-sentence. Both gate on the composer being the
    // thing on screen — an overlay is a different task than the commit
    // underneath it, and History isn't the composer at all.
    if (meta && (e.key === 'Enter' || e.key === 'g')) {
      if ($overlayDepth > 0 || $repoState.activeTab !== 'changes') return
      e.preventDefault()
      if (e.key === 'g') composer?.requestGenerate()
      else composer?.requestCommit()
      return
    }

    // ⌘R is the only route to a forced reload now that the sync ladder replaced
    // the Refresh button, so it sits above the `inField` bail like the
    // composer's own chords: a reload you have to leave the message box to ask
    // for is one nobody reaches. Nothing in a text field wants Ctrl+R either —
    // the one thing that does is the shell, and `isFromTerminal` returned above.
    if (meta && e.key === 'r') {
      e.preventDefault()
      void forceReload()
      return
    }

    if (inField) return

    if (meta && e.key === 'b') {
      e.preventDefault()
      if (showBranches) showBranches = false
      else openBranches()
    } else if (e.key === '?' && !meta) {
      e.preventDefault()
      showHelp = !showHelp
    } else if (meta && e.key === ',') {
      e.preventDefault()
      showSettings = !showSettings
    } else if (meta && e.key === 'l') {
      e.preventDefault()
      setActiveTab($repoState.activeTab === 'changes' ? 'history' : 'changes')
    } else if (meta && (e.key === '1' || e.key === '2')) {
      // Absolute, not a toggle: ⌘1 is always Changes and ⌘2 always History, so
      // the chord you press doesn't depend on the tab you're already on. The
      // native client binds the same pair in View, where the menu also carries
      // them as the app's own documentation.
      e.preventDefault()
      setActiveTab(e.key === '1' ? 'changes' : 'history')
    }
  }

  async function initialize() {
    await refreshConfig()
    // Seed the persisted sort-mode toggles and recents list before either
    // picker opens. Awaited so recordRecentRepo below prepends to the hydrated
    // list rather than racing the hydration that would otherwise clobber it.
    await hydrateReposState()
    // The repo we launched into counts as the most-recent open.
    const repoPath = $appState.repoPath
    if (repoPath) recordRecentRepo(repoPath)
    const [status] = await Promise.all([refreshStatus(), refreshBranches(), loadInitialLog()])
    if (status) lastHeadSha = status.head_sha
    statusLoop.start()
    fetchLoop.start()
    // Kick one immediate fetch of the open repo so the Pull "behind" badge
    // resolves within a second of launch instead of waiting up to a full
    // auto-fetch interval (and at all when auto-fetch is off). Non-blocking so
    // it never delays first paint. Mirrors the fetch-on-wake-up behaviour, and
    // it is why the loop's own start-up skew costs the user nothing.
    void performAutoFetch()
    // Background pull/push badges for the other recent repos in the picker.
    repoSyncScheduler.start()
  }

  $effect(() => {
    if ($repoState.activeTab === 'history' && !$repoState.log.loaded) {
      loadInitialLog()
    }
  })

  /**
   * Re-decide the automatic fetch's next run whenever its two settings move.
   *
   * Watching the config rather than the Settings dialog also covers a change
   * made in the native client, which arrives on the next wake-up through
   * `resyncOnActive`'s config re-read.
   *
   * Rescheduling rather than restarting is what makes "off" and "on" symmetric:
   * the interval is measured from the last fetch either way, so switching
   * auto-fetch off parks the loop where it stands, and switching it back on
   * runs it as soon as the *configured* interval says it is due — immediately
   * if it has been off for longer than that. The native client idles on a 30 s
   * re-check to notice the same change.
   */
  const fetchPolicy = $derived(`${$config?.auto_fetch}:${$config?.fetch_interval_ms}`)
  $effect(() => {
    void fetchPolicy
    untrack(() => fetchLoop.reschedule())
  })

  /*
    Re-read the open diffs when a setting the *read* depends on changes.

    `hide_whitespace` picks a different `git diff`; `side_by_side_diff` decides
    whether core builds the row pairing at all, so the layout the diff header
    just switched to has no rows of its own until this lands. Both panes
    reload, not only the changes one: whitespace hiding applies to working-tree
    diffs alone, but the layout applies to a commit's diff just the same.

    `force`, because the file has not changed — without it the loaders'
    "already open" short-circuit returns before reading, which is precisely the
    case this exists for.

    Derived key read by an `untrack`ed effect (the rule this client applies to
    anything reacting to a polled store): reading `repoState` inside the branch
    would re-run this on every status tick.
  */
  const diffReadKey = $derived(
    `${$config?.hide_whitespace ?? false}:${$config?.side_by_side_diff ?? false}`,
  )
  let lastDiffReadKey = $state<string | null>(null)
  $effect(() => {
    const key = diffReadKey
    untrack(() => {
      if (lastDiffReadKey === null || key === lastDiffReadKey) {
        lastDiffReadKey = key
        return
      }
      lastDiffReadKey = key
      const open = get(repoState)
      if (open.activeFile) void loadDiffForFile(open.activeFile, { force: true })
      if (open.activeCommitFile) void loadCommitFileDiff(open.activeCommitFile, { force: true })
    })
  })

  // Kill terminal PTY on project change so we don't leak shells from prior repos.
  let lastTerminalRepoPath = $state<string | undefined>(undefined)
  $effect(() => {
    const path = $appState.repoPath
    if (lastTerminalRepoPath === undefined) {
      lastTerminalRepoPath = path
      return
    }
    if (path !== lastTerminalRepoPath) {
      lastTerminalRepoPath = path
      activeShellLabel = ''
      terminalSessionId = 0
      terminalExpanded = false
    }
  })

  let unlistenOpenRepo: (() => void) | null = null
  let teardownConnectivity: (() => void) | null = null
  let teardownActivity: (() => void) | null = null

  onMount(() => {
    initialize().catch(console.error)
    // Live `leogit <dir>` switches while the app is open (see openExternalRepo).
    // A target that isn't a repository yet is App's to handle — it owns the
    // "create a repository here?" prompt for every phase.
    listen<LaunchTarget>('open-repo', (e) => {
      if (e.payload?.is_repo) openExternalRepo(e.payload.path).catch(console.error)
    }).then((u) => {
      unlistenOpenRepo = u
    })
    teardownActivity = observeActivity(handleActivityChange)
    window.addEventListener('keydown', handleKeyDown)
    // The moment the OS reports connectivity back, refresh the active repo and
    // the top picker tier immediately instead of waiting out the backoff window.
    teardownConnectivity = initConnectivity(() => {
      if ($appState.phase !== 'main') return
      void performAutoFetch()
      repoSyncScheduler.kickTopTier()
    })

    return () => {
      statusLoop.stop()
      fetchLoop.stop()
      repoSyncScheduler.stop()
      teardownConnectivity?.()
      teardownActivity?.()
      unlistenOpenRepo?.()
      window.removeEventListener('keydown', handleKeyDown)
    }
  })
</script>

<svelte:window onresize={() => (viewportVersion += 1)} />

<div class="main-layout" style="--sidebar-width: {sidebarWidth}px;">
  <!--
    The toolbar spans the window, above the sidebar/detail split, because that
    is where the native client's is: `ContentView`'s `.toolbar` (ContentView
    .swift:341) belongs to the *window*, so the repo chip, the branch chip and
    the sync control sit over the file list as much as over the diff. Hosting
    it inside `.main-content` started the bar at the split and left the sidebar
    wearing a bare tab strip — the single largest reason the two clients read
    as different windows.

    The wrapper is here only to be the grid item. `<Header>`'s own root belongs
    to that component's scope, so the `grid-column` that makes the bar span all
    three tracks has to be declared on a box this file owns.
  -->
  <div class="header-slot">
    <Header
      onOpenRepos={openRepos}
      onOpenBranches={openBranches}
      bind:repoChip
      bind:branchChip
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
      onTransferFinished={reloadAfterHeadMove}
    />
  </div>

  <div class="sidebar">
    <TabBar />
    <!--
      Both tab panes stay mounted and toggle via CSS so CommitMessage retains
      its in-progress draft (summary / description / co-authors) when the user
      switches to History and back. CommitList also keeps its scroll position.

      The wrapper is what gets measured for the composer's height cap: the
      Changes pane itself is `display: none` while History shows, and a pane
      reporting zero height would collapse the cap on every tab round trip.
    -->
    <div class="tab-panes" bind:clientHeight={tabPanesHeight}>
      <div class="tab-pane" class:active={$repoState.activeTab === 'changes'}>
        <div class="file-list-container">
          <FileList
            files={$repoState.status.files}
            selectedFiles={$repoState.selectedFiles}
            activeFile={$repoState.activeFile}
            contextActions={fileContextActions}
            onActivate={handleFileActivate}
            onToggle={handleFileToggle}
            onToggleAll={handleToggleAll}
            onBulkToggle={handleBulkToggle}
          />
        </div>
        <div class="commit-section" style="height: {effectiveCommitHeight}px;">
          <div
            class="commit-resize-handle"
            onmousedown={startCommitResize}
            onkeydown={handleCommitKey}
            role="slider"
            tabindex="0"
            aria-orientation="horizontal"
            aria-label="Resize commit section"
            aria-valuenow={effectiveCommitHeight}
            aria-valuemin={COMMIT_MIN}
            aria-valuemax={commitMax}
          ></div>
          <CommitMessage
            bind:this={composer}
            onCommitted={handleCommitted}
            onStopAmending={handleStopAmending}
            onRunInTerminal={runInTerminal}
          />
        </div>
      </div>
      <div class="tab-pane" class:active={$repoState.activeTab === 'history'}>
        <div class="commit-list-container">
          <CommitList
            commits={$repoState.log.commits}
            selectedSha={$repoState.activeCommit?.sha || null}
            unpushedShas={$repoState.status.unpushedShas}
            hasResolvedUpstream={$repoState.status.upstream !== ''}
            headSha={$repoState.status.headSha}
            resetSeq={$repoState.log.resetSeq}
            loaded={$repoState.log.loaded}
            onSelect={loadCommitFiles}
            onLoadMore={loadMoreCommits}
            onAmendCommit={handleStartAmending}
            onUndoCommit={handleUndoCommit}
            onCheckoutCommit={handleCheckoutCommit}
          />
        </div>
      </div>
    </div>
  </div>

  <div
    class="sidebar-resize-handle"
    onmousedown={startSidebarResize}
    onkeydown={handleSidebarKey}
    role="slider"
    tabindex="0"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    aria-valuenow={sidebarWidth}
    aria-valuemin={SIDEBAR_MIN}
    aria-valuemax={SIDEBAR_MAX}
  ></div>

  <div class="main-content">
    <!--
      Failures that aren't worth the window. Two of them share this strip and
      differ only in who retires them: the poll's own (a streak of failed ticks
      — the repo went away) clears the moment a tick succeeds, while an OS
      hand-off that failed has no later success to disprove it and so carries a
      ✕. Both leave the last good view of the repository readable behind them,
      which is the whole point — a modal on every tick hid it.
    -->
    {#if $repoState.pollError}
      <div class="poll-banner" role="status">
        <!-- Filled, not outlined: every warning banner on the native side uses
             `exclamationmark.triangle.fill` (`ContentView.swift:1055`), and the
             outlined variant is reserved there for the full-pane
             `ContentUnavailableView` states. -->
        <Icon name="exclamationmark-triangle-fill" size={13} />
        <span class="poll-banner-text">
          Can't read this repository — it may have been moved, deleted, or unmounted.
        </span>
        <span class="poll-banner-detail">{$repoState.pollError}</span>
      </div>
    {/if}

    {#if $repoState.notice}
      <div class="poll-banner" role="status">
        <Icon name="exclamationmark-triangle-fill" size={13} />
        <span class="banner-message">{$repoState.notice}</span>
        <button class="banner-dismiss" onclick={dismissNotice} aria-label="Dismiss">
          <Icon name="xmark" size={10} weight="semibold" />
        </button>
      </div>
    {/if}

    <div class="content-area">
      {#if $repoState.activeTab === 'changes'}
        <SeamlessDiffPane stale={$repoState.isDiffLoadingSlow}>
          {#if $repoState.activeFileDiffError}
            <!-- The read failed. Inline, where the diff would have been, and
                 with the stale payload already cleared by the loader — a
                 modal over the previous file's rows described one diff while
                 rendering another. Clicking the row again re-reads. -->
            <PaneEmptyState
              icon="exclamationmark-triangle"
              title="Couldn't Load Diff"
              verbatim={$repoState.activeFileDiffError}
            />
          {:else if $repoState.activeFile?.submodule_dirty}
            <!-- Submodule whose inner working tree is dirty but whose pointer
                 hasn't moved: the raw diff is just an opaque `Subproject commit
                 …-dirty` line, so we explain it instead, mirroring the checkbox
                 being disabled in the file list. -->
            <PaneEmptyState
              icon="arrow-turn-down-right"
              title="Submodule Changes"
              detail={"This submodule has modified content that hasn't been committed. Those " +
                'changes must be committed inside the submodule before they can be part of ' +
                'this repository.'}
            />
          {:else if hasRenderableDiff($repoState.activeFileDiff)}
            <DiffViewer
              diff={$repoState.activeFileDiff!}
              selection={null}
              blobSource={{ kind: 'workingTree', repoPath: $appState.repoPath }}
              origPath={$repoState.activeFile?.orig_path ?? null}
              showSelection={false}
              syntaxHighlighting={$config?.syntax_highlighting ?? true}
              sideBySide={$config?.side_by_side_diff ?? false}
              onLayoutChange={setDiffLayout}
              tabSize={$config?.tab_size ?? 4}
            />
          {:else if $repoState.activeFileDiff?.size_guard}
            <!-- Withheld rather than empty: rendering it would be slow, so the
                 pane explains and offers it instead of hanging on it. -->
            <PaneEmptyState
              icon="doc-text-magnifyingglass"
              title="Large Diff"
              detail={sizeGuardCopy($repoState.activeFileDiff.size_guard!)}
            >
              {#snippet actions()}
                <button onclick={() => showActiveDiffAnyway()}>Show Diff Anyway</button>
              {/snippet}
            </PaneEmptyState>
          {:else if $repoState.activeFile}
            <!--
              A file IS selected but there is nothing to render. Core says which
              of the three unrelated reasons it is; falling through to the
              no-selection copy below told the user to select the file they had
              already selected. Stays blank while the fetch is in flight so a
              sub-threshold load doesn't flash this state on its way to the diff.
            -->
            <div class="diff-empty-hold">
              {#if !$repoState.isDiffLoading}
                {@const copy = emptyDiffCopy($repoState.activeFileDiff)}
                <PaneEmptyState icon="doc" title={copy.title} detail={copy.detail} />
              {/if}
            </div>
          {:else}
            <!-- Two unrelated states, not one: nothing to select and nothing
                 selected read as different sentences and take different
                 glyphs — a clean tree is an outcome (`checkmark.circle`), an
                 unmade selection is an instruction (`doc.text`). -->
            {#if $repoState.status.files.length === 0}
              <PaneEmptyState
                icon="checkmark-circle"
                title="No Changes"
                detail="The working tree is clean."
              />
            {:else}
              <PaneEmptyState
                icon="doc-text"
                title="No File Selected"
                detail="Select a file to see its changes."
              />
            {/if}
          {/if}
        </SeamlessDiffPane>
      {:else if $repoState.activeCommit}
        <CommitDetail
          commit={$repoState.activeCommit}
          fileCount={$repoState.activeCommitFiles.length}
          stats={$repoState.activeCommitStats}
        />
        <div class="commit-body" style="--commit-files-width: {commitFilesWidth}px;">
          <div class="commit-files-pane">
            <FileList
              files={$repoState.activeCommitFiles}
              activeFile={$repoState.activeCommitFile}
              showCheckbox={false}
              onActivate={loadCommitFileDiff}
            />
          </div>
          <div
            class="commit-files-resize-handle"
            onmousedown={startCommitFilesResize}
            onkeydown={handleCommitFilesKey}
            role="slider"
            tabindex="0"
            aria-orientation="vertical"
            aria-label="Resize commit files pane"
            aria-valuenow={commitFilesWidth}
            aria-valuemin={COMMIT_FILES_MIN}
            aria-valuemax={COMMIT_FILES_MAX}
          ></div>
          <div class="commit-diff-pane">
            <SeamlessDiffPane stale={$repoState.isCommitDiffLoadingSlow}>
              {#if $repoState.activeCommitFileDiffError}
                <PaneEmptyState
                  icon="exclamationmark-triangle"
                  title="Couldn't Load Diff"
                  verbatim={$repoState.activeCommitFileDiffError}
                />
              {:else if hasRenderableDiff($repoState.activeCommitFileDiff)}
                <DiffViewer
                  diff={$repoState.activeCommitFileDiff!}
                  selection={null}
                  blobSource={$repoState.activeCommit
                    ? {
                        kind: 'commit',
                        repoPath: $appState.repoPath,
                        sha: $repoState.activeCommit.sha,
                      }
                    : null}
                  origPath={$repoState.activeCommitFile?.orig_path ?? null}
                  showSelection={false}
                  syntaxHighlighting={$config?.syntax_highlighting ?? true}
                  sideBySide={$config?.side_by_side_diff ?? false}
                  onLayoutChange={setDiffLayout}
                  tabSize={$config?.tab_size ?? 4}
                />
              {:else if $repoState.activeCommitFileDiff?.size_guard}
                <PaneEmptyState
                  icon="doc-text-magnifyingglass"
                  title="Large Diff"
                  detail={sizeGuardCopy($repoState.activeCommitFileDiff.size_guard!)}
                >
                  {#snippet actions()}
                    <button onclick={() => showActiveCommitDiffAnyway()}>Show Diff Anyway</button>
                  {/snippet}
                </PaneEmptyState>
              {:else if $repoState.activeCommitFile}
                <!-- Same split as the changes pane above: selected, but nothing
                     to render, and core names which reason. -->
                <div class="diff-empty-hold">
                  {#if !$repoState.isCommitDiffLoading}
                    {@const copy = emptyDiffCopy($repoState.activeCommitFileDiff)}
                    <PaneEmptyState icon="doc" title={copy.title} detail={copy.detail} />
                  {/if}
                </div>
              {:else}
                <PaneEmptyState
                  icon="doc-text"
                  title="No File Selected"
                  detail="Select a file to see its changes."
                />
              {/if}
            </SeamlessDiffPane>
          </div>
        </div>
      {:else if $repoState.log.loaded && $repoState.log.commits.length === 0}
        <!-- A repository with no history at all. Inviting the user to select a
             commit from a list that has none is an instruction they cannot
             follow, which is why this state exists separately from the one
             below it. It names the same fact the list beside it names, and
             deliberately so — `HistoryDetailPane.swift:29` and
             `HistorySidebar.swift:68` do the same — because the pane's job is
             to answer with the sentence the list has no room for. -->
        <PaneEmptyState
          icon="clock"
          title="No Commits Yet"
          detail="This repository has no commit history yet."
        />
      {:else}
        <PaneEmptyState
          icon="clock"
          title="No Commit Selected"
          detail="Select a commit to see its changes."
        />
      {/if}
    </div>

    {#if $appState.repoPath}
      <div class="terminal-section" class:collapsed={!terminalExpanded}>
        <div class="terminal-header">
          <button
            class="terminal-label"
            onclick={toggleTerminalMinimize}
            title={terminalExpanded ? 'Minimize terminal (Ctrl+`)' : 'Expand terminal (Ctrl+`)'}
            aria-label={terminalExpanded ? 'Minimize terminal' : 'Expand terminal'}
          >
            <Icon name="terminal" size={13} />
            <!-- The strip is always here, so it always says what it is. Before
                 a session exists there is no shell to name, and an unlabelled
                 glyph left the panel's whole purpose to be guessed at. -->
            <span class="shell-name">{activeShellLabel || 'Terminal'}</span>
          </button>
          <div class="terminal-controls">
            <button
              class="terminal-control-button"
              onclick={newTerminalSession}
              title="New terminal session"
              aria-label="New terminal session"
            >
              <Icon name="plus" weight="medium" />
            </button>
            <button
              class="terminal-control-button"
              onclick={toggleTerminalMinimize}
              title={terminalExpanded ? 'Minimize terminal (Ctrl+`)' : 'Expand terminal (Ctrl+`)'}
              aria-label={terminalExpanded ? 'Minimize terminal' : 'Expand terminal'}
            >
              <!-- A chevron in both directions rather than a minus collapsing
                   to a chevron: the native dock toggles between `chevron.down`
                   and `chevron.up` (`TerminalDock.swift:112`), so the pair
                   reads as one control pointing at where the panel will go. -->
              {#if terminalExpanded}
                <Icon name="chevron-down" weight="medium" />
              {:else}
                <Icon name="chevron-up" weight="medium" />
              {/if}
            </button>
            <button
              class="terminal-control-button close-button"
              onclick={killTerminalSession}
              title="Close terminal"
              aria-label="Close terminal"
              disabled={terminalSessionId === 0}
            >
              <Icon name="xmark" weight="medium" />
            </button>
          </div>
        </div>
        {#if terminalSessionId > 0}
          <div class="terminal-container">
            {#key `${$appState.repoPath}:${terminalSessionId}`}
              <Terminal
                bind:this={terminal}
                repoPath={$appState.repoPath}
                shellId={$config?.terminal_shell}
                expanded={terminalExpanded}
                onExit={killTerminalSession}
                onShellResolved={(label) => (activeShellLabel = label)}
              />
            {/key}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if showRepos}
    <div
      class="popover-layer"
      role="presentation"
      onclick={(e) => {
        if (e.target === e.currentTarget) showRepos = false
      }}
    >
      <div
        class="popover-frame arrowed"
        class:anchored={repoPlacement !== null}
        style={placementStyle(repoPlacement, REPO_POPOVER_WIDTH)}
        role="dialog"
        tabindex="-1"
      >
        <RepoDropdown
          repos={$appState.repos}
          currentRepo={$appState.repoPath}
          onSelect={handleSwitchRepo}
          onClone={openClone}
          onOpenSettings={() => { showRepos = false; showSettings = true }}
          onClose={() => (showRepos = false)}
        />
      </div>
    </div>
  {/if}

  <CloneOverlay
    isOpen={showClone}
    defaultDir={cloneDefaultDir}
    onClose={() => (showClone = false)}
    onCloned={handleCloned}
  />

  {#if showBranches}
    <div
      class="popover-layer"
      role="presentation"
      onclick={(e) => {
        if (e.target === e.currentTarget) showBranches = false
      }}
    >
      <div
        class="popover-frame"
        class:anchored={branchPlacement !== null}
        style={placementStyle(branchPlacement, BRANCH_POPOVER_WIDTH)}
        role="dialog"
        tabindex="-1"
      >
        <BranchDropdown
          branches={$repoState.branches}
          currentBranch={$repoState.status.branch}
          detached={$repoState.status.detached}
          merging={$repoState.status.isMerging}
          busy={branchOp !== null}
          onSwitch={handleSwitchBranch}
          onCreate={handleCreateBranch}
          onRequestMerge={requestMerge}
          onRequestDelete={requestDeleteBranch}
          onRequestAbortMerge={requestAbortMerge}
          onClose={() => (showBranches = false)}
        />
      </div>
    </div>
  {/if}

  {#if mergeSource}
    <MergeBranchDialog
      source={mergeSource}
      target={$repoState.status.branch}
      commitCount={mergeCommitCount}
      isMerging={branchOp === 'merge'}
      onMerge={() => void runMerge(false)}
      onSquashMerge={() => void runMerge(true)}
      onCancel={() => {
        if (branchOp !== 'merge') mergeSource = null
      }}
    />
  {/if}

  {#if deleteTarget}
    {@const branchName = deleteTarget}
    <ConfirmDialog
      title="Delete Branch?"
      confirmLabel="Delete"
      busyLabel="Deleting…"
      isBusy={branchOp === 'delete'}
      destructive
      onConfirm={() => void deleteBranch(branchName)}
      onCancel={() => {
        if (branchOp !== 'delete') deleteTarget = null
      }}
    >
      {#snippet body()}
        <p>Are you sure you want to delete <code>{branchName}</code>?</p>
        <p class="muted">Unmerged commits are lost.</p>
      {/snippet}
    </ConfirmDialog>
  {/if}

  {#if showAbortMerge}
    <ConfirmDialog
      title="Abort Merge?"
      confirmLabel="Abort Merge"
      busyLabel="Aborting…"
      isBusy={branchOp === 'abort'}
      destructive
      onConfirm={() => void abortMerge()}
      onCancel={() => {
        if (branchOp !== 'abort') showAbortMerge = false
      }}
    >
      {#snippet body()}
        <p>Abort the merge in progress?</p>
        <p class="muted">
          Conflict resolutions are discarded and the working tree returns to its pre-merge state.
        </p>
      {/snippet}
    </ConfirmDialog>
  {/if}

  <SettingsOverlay isOpen={showSettings} onClose={() => (showSettings = false)} />
  <HelpOverlay isOpen={showHelp} onClose={() => (showHelp = false)} />

  {#if discardTarget}
    <DiscardConfirm
      files={discardTarget}
      plan={discardPlan}
      {isDiscarding}
      error={discardError}
      onConfirm={confirmDiscard}
      onCancel={cancelDiscard}
    />
  {/if}

  {#if checkoutTarget}
    <CheckoutCommitConfirm
      commit={checkoutTarget}
      {isCheckingOut}
      onConfirm={confirmCheckout}
      onCancel={cancelCheckout}
    />
  {/if}

  {#if $repoState.error}
    <ErrorModal
      title="Error"
      message={$repoState.error}
      onDismiss={dismissActionError}
      onRetry={$repoState.errorRetry ? retryError : undefined}
    />
  {/if}
</div>

<style>
  .main-layout {
    display: grid;
    grid-template-columns: var(--sidebar-width, 320px) 1px 1fr;
    /* Two rows: the toolbar at its own height, everything else taking what is
       left. The `1fr` is what keeps the split panes bounded — they scroll
       inside a fixed viewport, so the row they sit in may not be sized by its
       contents. */
    grid-template-rows: auto 1fr;
    width: 100%;
    height: 100vh;
    background: var(--bg-primary);
    overflow: hidden;
  }

  /* Spanning all three tracks is the whole job; the sidebar, the resize handle
     and the detail pane then auto-place into row 2 in source order. */
  .header-slot {
    grid-column: 1 / -1;
    min-width: 0;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    overflow: hidden;
    min-height: 0;
  }

  .sidebar-resize-handle {
    position: relative;
    width: 1px;
    background: var(--border-inactive);
    cursor: col-resize;
    user-select: none;
    transition: background 120ms ease;
  }

  .sidebar-resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    right: -3px;
    z-index: 10;
  }

  .sidebar-resize-handle:hover,
  .sidebar-resize-handle:active {
    background: var(--border-active);
  }

  .commit-section {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 0;
    position: relative;
  }

  .commit-resize-handle {
    height: 4px;
    background: transparent;
    cursor: row-resize;
    user-select: none;
    flex-shrink: 0;
    /* The rule above the composer belongs to the handle, not to the composer:
       the native client's is a `Divider()` inside `RowResizeHandle`
       (RowResizeHandle.swift:41), and the composer itself draws nothing at its
       own edge. The handle is 4px of grab area *including* the line. */
    border-top: 1px solid var(--border-inactive);
    position: relative;
    z-index: 2;
  }

  .commit-resize-handle::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: -3px;
    bottom: -3px;
  }

  .commit-resize-handle:hover,
  .commit-resize-handle:active {
    background: var(--border-active);
  }

  .file-list-container,
  .commit-list-container {
    flex: 1;
    overflow: hidden;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /*
    Holds both panes so the space they share has a height even while the one
    being measured is hidden. Purely a measuring frame — it adds no box of its
    own beyond the flex column the panes were already in.
  */
  .tab-panes {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /*
    Both tab subtrees stay mounted (so CommitMessage doesn't drop its draft
    when the user peeks at History). Only the active one renders.
  */
  .tab-pane {
    display: none;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .tab-pane.active {
    display: flex;
  }

  .main-content {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    background: var(--bg-primary);
  }

  .content-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  /* Poll-failure strip: one line under the header, above the content, so the
     data behind it stays visible. Deliberately not a toast and not a modal —
     the condition persists, so it must be able to sit there. */
  .poll-banner {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-inactive);
    background: color-mix(in srgb, var(--status-yellow) 12%, transparent);
    color: var(--text-primary);
    font-size: 12px;
  }

  /* `:global` because the glyph is a child component's element now, and a
     direct-child combinator so it tints only the banner's own warning mark:
     the descendant form also caught the dismiss button's ✕ and beat
     `.banner-dismiss`'s colour on specificity, which is why that button's
     hover rule below could never fire. `flex-shrink` lives in `Icon`. */
  .poll-banner > :global(svg) {
    align-self: center;
    color: var(--status-yellow);
  }

  .poll-banner-text {
    flex-shrink: 0;
  }

  .poll-banner-detail {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    user-select: text;
  }

  /* The notice's message IS the sentence, not a technical footnote under one,
     so it reads at body weight rather than in the detail's muted mono. */
  .banner-message {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    user-select: text;
  }

  .banner-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    align-self: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .banner-dismiss:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .commit-body {
    flex: 1;
    display: grid;
    grid-template-columns: var(--commit-files-width, 280px) 1px 1fr;
    min-height: 0;
    overflow: hidden;
  }

  .commit-files-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-primary);
  }

  .commit-files-resize-handle {
    position: relative;
    background: var(--border-inactive);
    cursor: col-resize;
    z-index: 5;
  }

  .commit-files-resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    right: -3px;
  }

  .commit-files-resize-handle:hover,
  .commit-files-resize-handle:active {
    background: var(--border-active);
  }

  .commit-diff-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  /*
    Holds the pane open across a diff load that has not yet resolved into an
    empty state. The state itself is withheld until the read lands — most reads
    finish well inside the slow threshold, and a glyph-and-heading block that
    appears and vanishes on every quick file switch is worse than a still pane.

    So this carries nothing but the two declarations that stop the pane
    collapsing: `flex: 1` and the surface. `.content-area` paints no background
    of its own, so without the second one the loading pane shows through to
    whatever is behind it and the swap flashes anyway — for the same reason,
    just in the other direction.
  */
  .diff-empty-hold {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
  }

  .terminal-section {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    border-top: 1px solid var(--border-inactive);
    background: #000000;
  }

  .terminal-section.collapsed {
    background: var(--bg-secondary);
  }

  .terminal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 26px;
    padding: 0 6px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-inactive);
    flex-shrink: 0;
  }

  .terminal-section.collapsed .terminal-header {
    border-bottom: none;
  }

  .terminal-section.collapsed .terminal-container {
    display: none;
  }

  .terminal-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 20px;
    padding: 0 6px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: background 100ms ease, color 100ms ease;
  }

  .terminal-label:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  /* Which shell is running. Secondary to the panel controls, so it stays at
     the muted weight even while the label button is hovered. */
  .shell-name {
    font-size: 10px;
    white-space: nowrap;
  }

  .terminal-controls {
    display: flex;
    gap: 1px;
  }

  .terminal-control-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    line-height: 0;
    transition: background 100ms ease, color 100ms ease;
  }

  .terminal-control-button:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .terminal-control-button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .terminal-control-button.close-button:hover:not(:disabled) {
    background: var(--status-red);
    color: #ffffff;
  }

  /* 280px is the *emulator*, not the dock — the same meaning the native client
     gives the number, so both clients hand the shell the same number of rows.
     Measuring the dock instead spent the header's 26px out of the grid and left
     Tauri two rows short of native for the same setting. */
  .terminal-container {
    height: 280px;
    overflow: hidden;
    background: #000000;
  }

  /*
    The pickers' layer: a click-catcher that dismisses, never a scrim. A
    popover is transient, not modal, and the native dims nothing under one —
    `--overlay-backdrop` is the dialogs'. The centring is the fallback for a
    frame with no chip to hang from; an anchored frame ignores it and takes
    the coordinates `placeUnder` computed.
  */
  .popover-layer {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 60px;
    z-index: 1000;
  }

  .popover-frame {
    position: relative;
  }

  .popover-frame.anchored {
    position: fixed;
  }

  /*
    The popover's arrow, pointing back at the chip: a square turned 45° with
    the surface's own fill and hairline on its two upper edges, clipped to the
    triangle that stands above the box. Its lower half is not left to lie over
    the box — at this height it would reach the filter field — so a one-pixel
    strip of the same fill (the `::after`) covers the box's top border between
    the arrow's feet instead, which is all the overlap was ever for. Drawn on a
    frame this file owns because the dropdown clips its own overflow and could
    not draw outside itself. `--popover-arrow-x` is the chip's centre in the
    frame's coordinates — the box may have been pushed off centre by the
    window's edge, and the arrow must not go with it. The side is the height
    times √2: the square's diagonal is what stands up as the arrow, so its
    feet sit one height either side of the tip. `box-sizing` is stated because
    the universal reset does not reach pseudo-elements, and a content-box
    square would put the hairlines outside the side and the tip half a pixel
    off the chip's centre.
  */
  .popover-frame.arrowed.anchored::before {
    --arrow-side: calc(var(--popover-arrow-height) * 1.4142);
    content: '';
    position: absolute;
    z-index: 1;
    box-sizing: border-box;
    top: calc(var(--arrow-side) / -2);
    left: calc(var(--popover-arrow-x) - var(--arrow-side) / 2);
    width: var(--arrow-side);
    height: var(--arrow-side);
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-inactive);
    border-left: 1px solid var(--border-inactive);
    clip-path: polygon(0 0, 100% 0, 0 100%);
    transform: rotate(45deg);
    pointer-events: none;
  }

  .popover-frame.arrowed.anchored::after {
    content: '';
    position: absolute;
    z-index: 1;
    top: 0;
    left: calc(var(--popover-arrow-x) - var(--popover-arrow-height) + 1px);
    width: calc(var(--popover-arrow-height) * 2 - 2px);
    height: 1px;
    background: var(--bg-elevated);
    pointer-events: none;
  }
</style>

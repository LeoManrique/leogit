<script lang="ts">
  import { untrack } from 'svelte'
  import type { CommitInfo } from '$lib/api/commands'
  import {
    activeKey,
    applyGesture,
    clickGesture,
    contextTargets,
    isSelectAllChord,
    keyGesture,
    rowIndexForKey,
    selectAll,
    type ListSelection,
    type SelectionGesture,
  } from '$lib/utils/listSelection'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import { isTextInputElement } from '$lib/utils/focus'
  import { replaysMergeCommit } from '$lib/utils/historyRange'
  import { isFromTerminal } from '$lib/utils/keyboard'
  import {
    adjacentReorderSlot,
    canReorder,
    homeReorderSlot,
    reorderChangesNothing,
    reorderDestination,
    reorderSlots,
  } from '$lib/utils/reorderPlacement'
  import { focusVirtualRow, revealVirtualBand, revealVirtualRow } from '$lib/utils/virtualList'
  import ContextMenu, { MENU_SEPARATOR, type ContextMenuItem } from './ContextMenu.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    commits: CommitInfo[]
    /**
     * The highlighted rows, by sha — a **set**, because the history actions act
     * on several commits at once. Owned by the parent, which is what re-seats it
     * when a re-read drops the commits it named; this list only proposes the
     * next one, through `onSelect`. The rules are `listSelection.ts`'s, shared
     * with the changed-file list.
     */
    selection: ListSelection
    /** The one commit whose detail the pane shows; always one of `selection`. */
    activeSha: string | null
    unpushedShas?: Set<string>
    /**
     * Whether the backend was able to resolve an upstream ref (explicit or
     * inferred) for the current branch. Used to gate "Undo Last Commit" —
     * when an upstream is known, we only allow undo on unpushed commits;
     * when no upstream is resolvable (brand new repo, no remote), we can't
     * prove a commit is pushed, so we allow undo unconditionally on the top.
     */
    hasResolvedUpstream?: boolean
    /**
     * SHA of the repository's real HEAD, from `get_status`. Amend, undo and
     * checkout are gated on it — never on a row's index, which is an index
     * into the *loaded window* and stops meaning "HEAD" the moment that window
     * slides. Undo runs `git reset --mixed HEAD~1` against the real HEAD, so a
     * row-index gate offers it on the wrong commit and seeds the composer with
     * that commit's message (FRONTEND.md §6.10 forbids the index form).
     */
    headSha?: string
    /**
     * Bumped by the parent when it re-reads the list from HEAD (HEAD moved, a
     * different repo, the first load) instead of appending a page. Row 0 is
     * then a commit the user has not seen, and their scroll offset was
     * measured against a list whose top has changed — so we go to the top.
     */
    resetSeq?: number
    /**
     * Whether the initial history load has completed. Gates the "No commits
     * yet" empty state so it only shows for a genuinely empty repo (e.g. a
     * freshly initialized one), not during the brief window before the first
     * `get_log` resolves on a repo that does have commits.
     */
    loaded?: boolean
    /**
     * A gesture changed the selection. `active` is the commit the detail pane
     * should now show: the row the gesture landed on, which is this client's
     * rule where the native pane holds still for a multi-row selection
     * (FRONTEND §8).
     */
    onSelect: (selection: ListSelection, active: CommitInfo) => void
    onLoadMore: () => void
    onAmendCommit?: (commit: CommitInfo) => void
    onUndoCommit?: (commit: CommitInfo) => void
    onCheckoutCommit?: (commit: CommitInfo) => void
    /** Cherry-pick these commits — the menu's targets, newest first. */
    onCherryPick?: (commits: CommitInfo[]) => void
    /** Squash these commits into one — the menu's targets, newest first. */
    onSquash?: (commits: CommitInfo[]) => void
    /**
     * Move these commits — newest first — to just under `beforeSha`, or to the
     * tip for null. Called once a place has been chosen in the list.
     */
    onReorder?: (commits: CommitInfo[], beforeSha: string | null) => void
    /**
     * Why the actions that replay commits cannot start right now (an operation
     * in progress, a detached HEAD, another write running), or null when they
     * can. They stay in the menu disabled, with this as their hover text.
     */
    historyActionsBlocked?: string | null
  }

  let {
    commits = [],
    selection,
    activeSha = null,
    unpushedShas = new Set<string>(),
    hasResolvedUpstream = false,
    headSha = '',
    resetSeq = 0,
    loaded = true,
    onSelect,
    onLoadMore,
    onAmendCommit,
    onUndoCommit,
    onCheckoutCommit,
    onCherryPick,
    onSquash,
    onReorder,
    historyActionsBlocked = null,
  }: Props = $props()

  /** The list as it is drawn, by key — what every selection rule reads. */
  const shas = $derived(commits.map((c) => c.sha))

  /**
   * Land a gesture on a row and tell the parent, along with the commit the
   * detail pane should follow to. That is the row itself — except for a toggle
   * that took the row *out* of the selection, where the pane keeps the commit it
   * had while that is still selected and otherwise moves to the first selected
   * row.
   */
  function selectRow(commit: CommitInfo, gesture: SelectionGesture) {
    // Frozen while a reorder is choosing its place.
    if (reorder) return
    const next = applyGesture(selection, shas, commit.sha, gesture)
    const shown = next.keys.has(commit.sha) ? commit.sha : activeKey(next, shas, activeSha)
    // Nothing moved — a click on the one row already selected and showing.
    if (next === selection && shown === activeSha) return
    const active = shown === commit.sha ? commit : commits.find((c) => c.sha === shown)
    if (active) onSelect(next, active)
  }

  /**
   * The open menu: where it is, the row it was raised on, and the commits it
   * acts on. The targets are captured here rather than re-read from the
   * selection on click — a re-seat can land between the two, and the menu must
   * act on the commits whose count it printed.
   */
  let contextMenu = $state<{
    x: number
    y: number
    commit: CommitInfo
    targets: CommitInfo[]
  } | null>(null)

  /**
   * Right-click acts on the whole selection when it lands inside a multi-row
   * one, and otherwise selects the row it opens on first — so the menu and the
   * detail pane can never describe different commits. Native gets this from
   * `contextMenu(forSelectionType:)`; the rule here is `contextTargets`, the
   * file list's.
   *
   * The targets come back in list order, newest first: a set has no order, and
   * every history action names a *range*.
   */
  function openContextMenu(e: MouseEvent, commit: CommitInfo) {
    // Always ours: a right-click that raises nothing must not fall through to
    // the WebView's own menu.
    e.preventDefault()
    e.stopPropagation()
    const asked = new Set(contextTargets(selection, shas, commit.sha))
    // A multi-row selection keeps its rows — collapsing it to the clicked row
    // would throw away the very thing the menu is about to act on.
    if (asked.size <= 1) selectRow(commit, 'replace')
    const targets = asked.size > 1 ? commits.filter((c) => asked.has(c.sha)) : [commit]
    contextMenu = { x: e.clientX, y: e.clientY, commit, targets }
  }

  /**
   * Whether the right-clicked commit is the repository's actual HEAD. `headSha`
   * comes from `get_status`, so this stays true only for the commit the
   * rewriting actions would really act on — no matter where the loaded window
   * currently sits. An empty `headSha` (unborn branch, status not loaded yet)
   * matches nothing, which disables the rewriting actions rather than
   * mis-enabling them.
   */
  const isHeadCommit = $derived(
    contextMenu !== null && headSha !== '' && contextMenu.commit.sha === headSha,
  )

  /** A commit we can prove is *not* HEAD — the checkout target. Unknown HEAD
   *  disables it too: detaching onto the commit you are already on is a
   *  surprise, not a no-op. */
  const isPastCommit = $derived(
    contextMenu !== null && headSha !== '' && contextMenu.commit.sha !== headSha,
  )

  /**
   * Cherry-pick, worded for how many commits it acts on. One item for both
   * menus: a single commit is the same machinery with N = 1.
   */
  const cherryPickItem = $derived.by<ContextMenuItem>(() => {
    const count = contextMenu?.targets.length ?? 1
    return {
      label: count > 1 ? `Cherry-pick ${count} Commits…` : 'Cherry-pick Commit…',
      enabled: onCherryPick !== undefined && historyActionsBlocked === null,
      title: historyActionsBlocked ?? undefined,
      action: () => {
        if (contextMenu) onCherryPick?.(contextMenu.targets)
      },
    }
  })

  /** The menu's targets as the set the range helpers read. */
  const targetShas = $derived(new Set((contextMenu?.targets ?? []).map((c) => c.sha)))

  /**
   * Why the actions that *replay* this branch cannot run on the menu's targets:
   * the shared gate, then a merge commit anywhere from HEAD down to the oldest
   * of them — read off the rows, which are all loaded from HEAD down to any
   * selected one. Cherry-pick is not one of these: it replays nothing here.
   */
  const replayBlocked = $derived(
    historyActionsBlocked ??
      (replaysMergeCommit(commits, targetShas)
        ? 'A merge commit is among the commits this would replay'
        : null),
  )

  const squashItem = $derived.by<ContextMenuItem>(() => {
    const targets = contextMenu?.targets ?? []
    return {
      label: `Squash ${targets.length} Commits…`,
      enabled: onSquash !== undefined && replayBlocked === null,
      title: replayBlocked ?? undefined,
      action: () => {
        if (contextMenu) onSquash?.(contextMenu.targets)
      },
    }
  })

  /**
   * Reorder has no dialog: its item arms the list's insertion mode. Off under
   * the replay gate, and when the commits have nowhere to go — the only commit
   * of the branch, or everything above a merge.
   */
  const reorderItem = $derived.by<ContextMenuItem>(() => {
    const targets = contextMenu?.targets ?? []
    const blocked =
      replayBlocked ?? (canReorder(commits, targetShas) ? null : 'There is nowhere to move to')
    return {
      label: targets.length > 1 ? `Reorder ${targets.length} Commits…` : 'Reorder Commit…',
      enabled: onReorder !== undefined && blocked === null,
      title: blocked ?? undefined,
      action: () => {
        if (contextMenu) armReorder(contextMenu.targets)
      },
    }
  })

  const singleCommitItems = $derived<ContextMenuItem[]>(
    contextMenu === null
      ? []
      : [
          {
            label: 'Amend Last Commit…',
            // Only HEAD can be amended without rewriting earlier history.
            enabled: isHeadCommit && onAmendCommit !== undefined,
            action: () => {
              if (contextMenu) onAmendCommit?.(contextMenu.commit)
            },
          },
          {
            // No ellipsis: undo runs immediately, and nothing is lost that the
            // composer and the working tree don't now hold. The ellipsis its
            // neighbours carry is a promise of a dialog, and this one never
            // had a dialog to open.
            label: 'Undo Last Commit',
            // HEAD only, and only when we believe it's still local — either we
            // can prove it's unpushed, or we couldn't resolve an upstream at
            // all (so we can't prove it's pushed either).
            enabled:
              isHeadCommit &&
              onUndoCommit !== undefined &&
              (!hasResolvedUpstream || unpushedShas.has(contextMenu.commit.sha)),
            action: () => {
              if (contextMenu) onUndoCommit?.(contextMenu.commit)
            },
          },
          {
            // The other half of the same rule: this one *does* confirm first,
            // so it says so.
            label: 'Check Out Commit…',
            // Any commit except the current HEAD — checking out HEAD is a
            // no-op. Lands the user in a detached HEAD.
            enabled: isPastCommit && onCheckoutCommit !== undefined,
            action: () => {
              if (contextMenu) onCheckoutCommit?.(contextMenu.commit)
            },
          },
          cherryPickItem,
          reorderItem,
          MENU_SEPARATOR,
          {
            label: 'Copy SHA',
            action: () => {
              if (contextMenu) copySha(contextMenu.commit)
            },
          },
          {
            label: 'Copy Tag',
            // Only meaningful when the commit actually carries a tag.
            enabled: contextMenu.commit.tags.length > 0,
            action: () => {
              if (contextMenu) copyTag(contextMenu.commit)
            },
          },
        ],
  )

  // A multi-row selection offers only what acts on all of it: every
  // single-commit item would have to pick one row to mean.
  const menuItems = $derived<ContextMenuItem[]>(
    contextMenu !== null && contextMenu.targets.length > 1
      ? [cherryPickItem, squashItem, reorderItem]
      : singleCommitItems,
  )

  // ---- Reorder: the insertion mode ------------------------------------------

  /**
   * The reorder choosing its place: the commits the menu armed it on — captured
   * like the menu's own targets, since the live selection can be re-seated
   * underneath — the slot the line is in, and the read of the list it was armed
   * over. Null when the mode is not armed. The slots are `reorderPlacement.ts`'s.
   */
  let reorder = $state<{ moving: CommitInfo[]; slot: number; armedAt: number } | null>(null)
  /** The key caption, up for a few seconds or until the first arrow. */
  let reorderHintShown = $state(false)

  const reorderMoving = $derived(new Set(reorder?.moving.map((c) => c.sha) ?? []))
  const reorderSlotsNow = $derived(reorder ? reorderSlots(commits, reorderMoving) : [])

  function armReorder(moving: CommitInfo[]) {
    const slot = homeReorderSlot(commits, new Set(moving.map((c) => c.sha)))
    reorder = { moving, slot, armedAt: resetSeq }
    reorderHintShown = true
    revealReorderSlot(slot)
  }

  function cancelReorder() {
    reorder = null
  }

  /** The line arrives with a row on each side of it. */
  function revealReorderSlot(slot: number) {
    revealVirtualBand({
      container: scrollContainer,
      top: Math.max(0, slot - 1) * ROW_HEIGHT,
      height: ROW_HEIGHT * 2,
      onScroll: (top) => (scrollTop = top),
    })
  }

  function moveReorderLine(step: -1 | 1) {
    if (!reorder) return
    reorderHintShown = false
    const slot = adjacentReorderSlot(reorderSlotsNow, reorder.slot, step)
    reorder = { ...reorder, slot }
    revealReorderSlot(slot)
  }

  /**
   * ⏎: the mode ends either way, and the move is asked for unless the line is
   * where the commits already are — which is the user deciding to leave them.
   */
  function confirmReorder() {
    if (!reorder) return
    const { moving, slot } = reorder
    // Judged before the mode ends: `reorderMoving` is derived from it, and
    // reads as nothing once it is null.
    const staysPut = reorderChangesNothing(commits, reorderMoving, slot)
    reorder = null
    if (!staysPut) onReorder?.(moving, reorderDestination(commits, slot))
  }

  // While armed the keys are the line's wherever focus is — the menu item that
  // armed it took focus with it as it unmounted. Escape is not here: it goes
  // through the overlay stack like every other dismissal (the caption
  // registers), so a dialog on top would get it first. A chord is somebody
  // else's (⌘↩ is the composer's), and so is a key typed into the terminal or a
  // field that Tab reached.
  $effect(() => {
    if (!reorder) return
    function onKeyDown(e: KeyboardEvent) {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (isFromTerminal(e) || isTextInputElement(e.target)) return
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault()
        e.stopPropagation()
        moveReorderLine(e.key === 'ArrowUp' ? -1 : 1)
      } else if (e.key === 'Enter') {
        e.preventDefault()
        e.stopPropagation()
        confirmReorder()
      }
    }
    // Any click ends it, either button, wherever it lands. One inside the list
    // does nothing else — no selection, no menu, the WebView's own included —
    // and one outside goes on to whatever it was aimed at.
    function onClick(e: MouseEvent) {
      if (e.target instanceof Node && scrollContainer?.contains(e.target)) {
        e.preventDefault()
        e.stopPropagation()
      }
      cancelReorder()
    }
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('click', onClick, true)
    window.addEventListener('contextmenu', onClick, true)
    return () => {
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('click', onClick, true)
      window.removeEventListener('contextmenu', onClick, true)
    }
  })

  // The caption retires by itself. Keyed on armed-or-not, never on the slot, or
  // every arrow would restart the clock.
  const reorderArmed = $derived(reorder !== null)
  $effect(() => {
    if (!reorderArmed) return
    const id = setTimeout(() => (reorderHintShown = false), REORDER_HINT_MS)
    return () => clearTimeout(id)
  })

  // What ends the mode from outside: the list was re-read from HEAD (a commit,
  // another repository — the rows under the line are not the ones it was armed
  // over), a moved commit left it, the line's slot stopped existing, the
  // actions became blocked, or the pane went away (Changes took over —
  // `containerHeight` is 0 then). Paging only adds slots below, and ends nothing.
  $effect(() => {
    if (!reorder) return
    const gone =
      reorder.armedAt !== resetSeq ||
      reorder.moving.some((c) => !shas.includes(c.sha)) ||
      !reorderSlotsNow.includes(reorder.slot) ||
      historyActionsBlocked !== null ||
      containerHeight === 0
    if (gone) cancelReorder()
  })

  // Built from the native row rather than chosen, the same way `FileList`
  // derives its 30. `CommitRow` is a `VStack(spacing: 2)`
  // (`HistorySidebar.swift:194`) over two lines — the summary line, whose
  // height is the taller of its 13pt text (16) and its 16pt tag chips
  // (`:220`, `:247`), and the `.caption` line beneath, which macOS draws at
  // 10pt and the engine gives a 12px line box — under `.padding(.vertical, 3)`
  // (`:231`). That is 3 + 16 + 2 + 12 + 3 = 36pt of row content. The `List`
  // holding it is `.listStyle(.inset)` (`:108`), which adds 4pt above and
  // below every row on macOS, and its `intercellSpacing` height is 0 on
  // Big Sur and later, so rows abut and the pitch *is* the row height:
  // 36 + 8 = 44.
  //
  // The one point of slack in that sum is the caption's line box: AppKit gives
  // a 10pt caption 13, and the engine's `normal` gives it 12, because `normal`
  // rounds SF's ascent and descent separately and loses a point below 12pt. The
  // only way to close it is a pinned `line-height` on a single-line label,
  // which is exactly what STYLE.md's leading rule forbids — so the row keeps
  // the engine's box and centres the pair inside it, which spends the point as
  // half a pixel of air at each end.
  //
  // The number is the whole of the row's proportion, and 6pt too many turns a
  // sidebar of commits into a table of records, so it is derived and not tuned.
  // Must stay in sync with `.commit-row { height }`, which the virtualizer
  // positions by.
  const ROW_HEIGHT = 44
  const VISIBLE_ROWS = 14
  /** How long the reorder caption stays up when no arrow retires it first. */
  const REORDER_HINT_MS = 4000
  const LOAD_MORE_OFFSET = 200

  let containerHeight = $state(ROW_HEIGHT * VISIBLE_ROWS)
  let scrollTop = $state(0)
  let scrollContainer = $state<HTMLElement>()

  function handleScroll(e: Event) {
    const target = e.target as HTMLElement
    scrollTop = target.scrollTop
    const scrollDist = target.scrollHeight - target.scrollTop - target.clientHeight
    if (scrollDist < LOAD_MORE_OFFSET) {
      onLoadMore()
    }
  }

  /**
   * Move keyboard focus and the selection to another row (Shift extends), then
   * bring it into view and focus it so the next arrow press continues from
   * there — the file list's `focusRowAt`.
   */
  async function focusRowAt(index: number, gesture: SelectionGesture) {
    const next = commits[index]
    if (!next) return
    selectRow(next, gesture)
    await focusVirtualRow({
      container: scrollContainer,
      index,
      rowHeight: ROW_HEIGHT,
      rowSelector: `[data-commit-row-index="${index}"]`,
      onScroll: (top) => (scrollTop = top),
    })
  }

  function getVisibleRange() {
    const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 5)
    const endIndex = Math.min(commits.length, Math.ceil((scrollTop + containerHeight) / ROW_HEIGHT) + 5)
    return { startIndex, endIndex }
  }

  // A ticking "now" so the relative dates stay live while History is open:
  // formatDate() reads this, so bumping it re-renders the visible "N minutes
  // ago" labels. The 10 s cadence stays effectively free because the work is
  // gated on visibility — we skip the tick entirely when the History pane is
  // hidden (Changes tab → clientHeight 0) or the window is backgrounded — and
  // the list is virtualized, so an on-screen tick only re-renders the ~14
  // mounted rows. The interval is torn down with the component.
  let now = $state(Date.now())
  $effect(() => {
    const id = setInterval(() => {
      if (document.hidden || containerHeight === 0) return
      now = Date.now()
    }, 10_000)
    return () => clearInterval(id)
  })

  // Relative timestamp for every commit, regardless of age. Tiered so old
  // commits read as "5 months ago" instead of an absolute date the user has to
  // mentally diff against today.
  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    const diffMs = now - date.getTime()
    const mins = Math.floor(diffMs / 60_000)
    const hours = Math.floor(diffMs / 3_600_000)
    const days = Math.floor(diffMs / 86_400_000)
    const months = Math.floor(days / 30)
    const years = Math.floor(days / 365)

    if (mins < 1) return 'just now'
    if (mins < 60) return mins === 1 ? '1 minute ago' : `${mins} minutes ago`
    if (hours < 24) return hours === 1 ? '1 hour ago' : `${hours} hours ago`
    if (days < 30) return days === 1 ? '1 day ago' : `${days} days ago`
    if (years < 1) return months === 1 ? '1 month ago' : `${months} months ago`
    return years === 1 ? '1 year ago' : `${years} years ago`
  }

  // Absolute local time for the row tooltip, which is where the exact date
  // lives now that the row itself only states an age. Abbreviated month and a
  // minute-precision time — `CommitDate.absolute`'s
  // `.formatted(date: .abbreviated, time: .shortened)`, the same string the
  // native row hands `.help()` (`HistorySidebar.swift:232`) and the same one
  // the detail card prints. `dateStyle: 'full'` spelled the weekday and month
  // out in words, which is a different sentence from the one the reference
  // shows and from the one two panes away.
  function formatDateAbsolute(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    })
  }

  function handleRowKeyDown(e: KeyboardEvent, commit: CommitInfo, index: number) {
    // The arrows and Return are the insertion line's while it is up.
    if (reorder) return
    const target = rowIndexForKey(e, index, commits.length)
    if (target !== null) {
      // The container would scroll otherwise; move the selection instead.
      e.preventDefault()
      void focusRowAt(target, keyGesture(e))
    } else if (isSelectAllChord(e)) {
      // Every *loaded* commit — the rows that exist to be selected. The pane
      // holds still: the commit it shows is among them.
      e.preventDefault()
      const all = selectAll(selection, shas)
      const active = commits.find((c) => c.sha === activeKey(all, shas, activeSha))
      if (active) onSelect(all, active)
    } else if (e.key === 'Enter' || e.key === ' ') {
      // Activation, for a row reached by Tab and so focused without being
      // selected. On a row that already is, it does nothing: Space is the file
      // list's bulk toggle, and a hand that presses it here out of habit must
      // not collapse the selection it just built.
      e.preventDefault()
      if (!selection.keys.has(commit.sha)) selectRow(commit, 'replace')
    }
  }

  async function copySha(commit: CommitInfo) {
    try {
      await navigator.clipboard.writeText(commit.sha)
    } catch {}
  }

  // `commit.tags` comes pre-split from the backend's %D parsing, so the rows
  // render tag pills straight off the payload.
  async function copyTag(commit: CommitInfo) {
    if (commit.tags.length === 0) return
    try {
      // Space-separate when a commit carries more than one tag.
      await navigator.clipboard.writeText(commit.tags.join(' '))
    } catch {}
  }

  // Track the viewport height with a ResizeObserver, not a one-shot read. The
  // History pane is display:none while the Changes tab is active, so measuring
  // clientHeight once at mount can capture 0 and strand the virtual window at
  // ~5 rows (ceil(0 / ROW_HEIGHT) + buffer). The observer fires when the pane
  // gains size, keeping containerHeight (and thus the rendered range) correct.
  $effect(() => {
    const el = scrollContainer
    if (!el) return
    const ro = new ResizeObserver(() => {
      const h = el.clientHeight
      // Pane just became visible again (Changes → History): refresh the dates
      // immediately instead of waiting up to 10 s for the next gated tick.
      if (h > 0 && containerHeight === 0) now = Date.now()
      containerHeight = h
    })
    ro.observe(el)
    containerHeight = el.clientHeight
    return () => ro.disconnect()
  })

  const { startIndex, endIndex } = $derived.by(() => getVisibleRange())
  const visibleCommits = $derived(commits.slice(startIndex, endIndex))
  const offsetPx = $derived(startIndex * ROW_HEIGHT)

  /*
    Go to the top when the parent re-reads the list from HEAD.

    Paging needs no counterpart: rows are only ever appended past the ones on
    screen, so every visible row keeps its position and the viewport is already
    where the user left it. A re-read is the only move that changes what row 0
    *is* — and row 0 is then the commit they just made, checked out or undid,
    which is what they should be looking at.

    Reading `resetSeq` inside the effect makes it the dependency; the previous
    value lives in $state so the change is observable across runs without
    looping.
  */
  let lastResetSeq = $state<number | null>(null)
  $effect(() => {
    const seq = resetSeq
    if (lastResetSeq === seq) return
    const first = lastResetSeq === null
    lastResetSeq = seq
    if (first || !scrollContainer) return
    scrollContainer.scrollTop = 0
    scrollTop = 0
  })

  /*
    Bring the pane's commit into view whenever it changes. A click lands on a
    row already in view and the keyboard scrolls for itself, so both are no-ops
    here; this is for a selection made in code — the commits a History action
    just produced, which can sit anywhere in the branch (FRONTEND §6.21).

    Declared after the go-to-top effect on purpose: an action re-reads the list
    and then selects, and when both land in one flush this one runs last. Only
    `activeSha` is a dependency — paging and re-reads must not drag the
    viewport back to a selection the user has scrolled away from.
  */
  $effect(() => {
    const sha = activeSha
    if (sha === null) return
    untrack(() => {
      revealVirtualRow({
        container: scrollContainer,
        index: shas.indexOf(sha),
        rowHeight: ROW_HEIGHT,
        onScroll: (top) => (scrollTop = top),
      })
    })
  })
</script>

<div class="commit-list-frame">
  <div
    class="commit-list"
    style="--row-height: {ROW_HEIGHT}px"
    bind:this={scrollContainer}
    onscroll={handleScroll}
  >
    {#if loaded && commits.length === 0}
      <div class="empty-state">
        <p>No commits yet</p>
      </div>
    {/if}
    <div class="virtual-scroll" style="height: {commits.length * ROW_HEIGHT}px">
      {#if reorder}
        <!--
          Beside `.visible-items`, not inside it: that wrapper is translated by
          the virtualizer, and the spacer is the one element whose coordinates
          are the model's — the gap above row N is exactly N rows down.
        -->
        <div class="insertion-line" style="top: {reorder.slot * ROW_HEIGHT}px"></div>
      {/if}
      <div class="visible-items" style="transform: translateY({offsetPx}px)">
        {#each visibleCommits as commit, i (commit.sha)}
          {@const rowIndex = startIndex + i}
          {@const tags = commit.tags}
          {@const isUnpushed = unpushedShas.has(commit.sha)}
          <div
            class="commit-row"
            class:selected={commit.sha === activeSha}
            class:row-selected={selection.keys.has(commit.sha)}
            class:striped={rowIndex % 2 === 1}
            data-commit-row-index={rowIndex}
            title={formatDateAbsolute(commit.author_date)}
            onclick={(e) => selectRow(commit, clickGesture(e))}
            oncontextmenu={(e) => openContextMenu(e, commit)}
            onkeydown={(e) => handleRowKeyDown(e, commit, rowIndex)}
            role="button"
            tabindex="0"
          >
            <div class="summary-line">
              <span class="commit-summary">{commit.summary}</span>
              {#if tags.length > 0 || isUnpushed}
                <div class="commit-indicators">
                  {#if tags.length > 0}
                    <span class="tag-indicator" title={tags.join(', ')}>
                      <span class="tag-name">{tags[0]}</span>
                      {#if tags.length > 1}
                        <span class="tag-indicator-more">+{tags.length - 1}</span>
                      {/if}
                    </span>
                  {/if}
                  {#if isUnpushed}
                    <span class="unpushed-badge" title="Not yet pushed" aria-label="Not yet pushed">
                      <!-- `bold` on purpose, not for emphasis: the native draws
                           this same marker at `.system(size: 9, weight: .bold)`
                           (`HistorySidebar.swift:217`), and a symbol's stroke
                           tracks the weight of the text it sits with. -->
                      <Icon name="arrow-up" size={10} weight="bold" />
                    </span>
                  {/if}
                </div>
              {/if}
            </div>
            <!--
              One text run, not three spans in a flex row. The native row's second
              line is a single interpolated `Text` — `"\(authorName) · \(relative)"`
              (`HistorySidebar.swift:226`, and the byte is U+00B7 with one ordinary
              space either side) — so the separator is worth about 2.5px of space
              at this size, where a flex `gap` would put its own value there twice
              and visibly widen the line.

              It also settles what gives way when the sidebar narrows: one run
              under `.lineLimit(1)` (`:229`) truncates at the tail, so the date is
              what goes, not the author.
            -->
            <div class="meta-line">{commit.author_name} · {formatDate(commit.author_date)}</div>
          </div>
        {/each}
      </div>
    </div>
  </div>
  {#if reorder}
    <!-- Registered with the overlay stack for as long as the mode is armed, which
         is what makes Escape cancel it — and keeps the app's chords quiet. -->
    <div
      class="reorder-hint"
      class:retired={!reorderHintShown}
      role="status"
      use:dismissOnEscape={cancelReorder}
    >
      ↑ ↓ choose a position · ⏎ move · esc cancel
    </div>
  {/if}
</div>

<!--
  ContextMenu MUST be rendered outside `.visible-items` because that wrapper
  has `transform: translateY(...)` for virtual scrolling, which establishes a
  containing block for any `position: fixed` descendant and would offset the
  menu off-screen.
-->
{#if contextMenu !== null}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    items={menuItems}
    onClose={() => (contextMenu = null)}
  />
{/if}

<style>
  /* The scroller's frame: what the reorder caption is pinned to, so that it
     stays put while the rows move under it. */
  .commit-list-frame {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .commit-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--bg-primary);
    border-right: 1px solid var(--border-inactive);
    padding: 4px 6px;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-faint);
    font-size: 13px;
  }

  .virtual-scroll {
    position: relative;
  }

  .visible-items {
    will-change: transform;
  }

  /* Where the moved commits will land. Centred on the join between two rows —
     a pixel from each — and drawn over them; at either end of the list it sits
     in the scroller's own 4px of padding. The accent is the affordance rule's
     (STYLE.md): this line is the thing the arrows move. */
  .insertion-line {
    position: absolute;
    z-index: 1;
    left: 0;
    right: 0;
    height: 2px;
    margin-top: -1px;
    border-radius: 1px;
    background: var(--border-active);
    pointer-events: none;
  }

  /* The keys, said once: the elevated plate the popovers wear, at the caption
     register, laid over the foot of the list. It retires after a few seconds or
     at the first arrow — the line itself is what says the mode is armed. */
  .reorder-hint {
    position: absolute;
    bottom: 10px;
    left: 50%;
    transform: translateX(-50%);
    max-width: calc(100% - 20px);
    padding: 4px 10px;
    border: 1px solid var(--border-inactive);
    border-radius: 6px;
    background: var(--bg-elevated);
    box-shadow: var(--shadow-popover);
    color: var(--text-secondary);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
    animation: reorder-hint-in 160ms ease;
    transition: opacity 160ms ease;
  }

  .reorder-hint.retired {
    opacity: 0;
  }

  @keyframes reorder-hint-in {
    from {
      opacity: 0;
    }
  }

  /* Under Reduce Motion it appears and goes without the fade. */
  @media (prefers-reduced-motion: reduce) {
    .reorder-hint {
      animation: none;
      transition: none;
    }
  }

  .commit-row {
    display: flex;
    flex-direction: column;
    justify-content: center;
    /* `VStack(alignment: .leading, spacing: 2)` (`HistorySidebar.swift:194`).
       The two lines sit 2px apart and the row's remaining slack — the native
       row's own 3pt of vertical padding plus the inset `List`'s 4pt above and
       below — falls either side of the pair, which is what centring does. */
    gap: 2px;
    /* Published by the wrapper from `ROW_HEIGHT`, which is also the step the
       virtualizer positions by — one number, so they cannot drift. */
    height: var(--row-height);
    padding: 0 10px;
    /* Apple's own sample for imitating list selection draws it as
       `.rect(cornerRadius: 6)`, and `NSTableView.Style.inset` is documented to
       round the row background and its selection together. */
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    transition: background 100ms ease;
    user-select: none;
    overflow: hidden;
  }

  /*
    Alternating row backgrounds, as the native History list has
    (`HistorySidebar.swift:109` — the same `.alternatingRowBackgrounds()` the
    changed-file list calls, and the reason both Tauri lists stripe or neither
    does). Row 0 is the plain one.

    Keyed on the commit's index in the model, never on DOM position: this list
    is virtualized and its rows are translated as a block, so `:nth-child` sees
    only the slice near the viewport and would restripe the whole list on every
    scroll. `FileList.svelte` carries the same rule for the same reason, and
    the alpha is deliberately about half `--surface-hover` — AppKit's own ~4.7 %
    would land close enough to the hover fill to cost a state these lists have
    and the native ones do not.

    Declared before the three state fills: all four weigh (0,2,0), so source
    order is the whole of the cascade here and a stripe written later would
    paint over the row the pointer is on.
  */
  .commit-row.striped {
    background: var(--surface-stripe);
  }

  .commit-row:hover {
    background: var(--surface-hover);
  }

  /*
    Selection is a set (STYLE.md, *Changes list*): every highlighted row takes
    the lighter tint, and the one row whose detail is showing keeps the heavier
    plate — `FileList.svelte`'s `.row-selected` and `.active`, in that order for
    the same source-order reason as the stripe above.
  */
  .commit-row.row-selected {
    background: var(--surface-hover);
  }

  .commit-row.selected {
    background: var(--bg-tertiary);
  }

  /* ── Line 1: summary + tag / push indicators ──
     6px between every item on this line, including between the summary and the
     first chip: the native row builds it as one `HStack(spacing: 6)` whose
     chips and unpushed plate are all direct children
     (`HistorySidebar.swift:195`), so a single spacing governs the whole run. */
  .summary-line {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: 6px;
  }

  .commit-summary {
    /* Grow to fill the row so indicators are pushed to the right edge, and
       shrink with ellipsis when the message is long. */
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-size: 13px;
    min-width: 0;
  }

  .commit-indicators {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 50%;
  }

  /* The tag and its `+N` companion are separate children of the native row's
     one `HStack(spacing: 6)`, so they sit 6px apart like everything else on
     the line — this wrapper only exists to keep them together when the
     indicator cluster shrinks. */
  .tag-indicator {
    flex: 0 1 auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  /*
    Both chips come out of one native builder — `chip(_:)` at
    `HistorySidebar.swift:242-250`: 10.5px mono, 5px of horizontal padding,
    a pinned 16px height and a 5px radius on the `.quaternary` plate that
    `--badge-bg` / `--badge-fg` stand for. `+N` is that same builder called
    with a different string (`:207`), so it takes the same type and the same
    padding rather than a smaller register of its own.

    `line-height: 16px` here is the pinned-box exception STYLE.md allows: it is
    the chip's geometry, matching `.frame(height: 16)`, not reading leading.
  */
  .tag-name,
  .tag-indicator-more {
    height: 16px;
    line-height: 16px;
    padding: 0 5px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-indicator-more {
    flex: 0 0 auto;
  }

  /*
    Unpushed-commit badge. Shares the tag badge's pill family — same 16px
    height, 5px radius, neutral --badge-bg/--badge-fg — so the two indicators
    read as one consistent set (mirrors the inspo app, where the unpushed
    indicator reuses the tag badge background). The up-arrow inside marks the
    commit as "not yet pushed", far more visible than the old bare faint icon.
  */
  .unpushed-badge {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    /* A fixed square, not a padded minimum: the native plate is
       `.frame(width: 16, height: 16)` (`HistorySidebar.swift:220`), so it is
       exactly as wide as it is tall whatever the glyph inside measures. These
       two indicators are meant to read as one family, and a plate wider than
       the chip beside it is tall is the one proportion that breaks that. */
    width: 16px;
    height: 16px;
    border-radius: 5px;
    background: var(--badge-bg);
    color: var(--badge-fg);
  }

  /* ── Line 2: author · relative date ──
     `.font(.caption).foregroundStyle(.secondary)` on the native run
     (`HistorySidebar.swift:227-228`): macOS draws `.caption` at 10pt regular,
     and `.secondary` is `--text-secondary`, one step brighter than the
     `--text-muted` that stands for `.tertiary`. Tabular digits come from the
     app-wide `body` rule, so a ticking "N minutes ago" still can't wobble. */
  .meta-line {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-size: 10px;
  }
</style>

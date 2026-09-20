import SwiftUI

/// The History tab's half of the sidebar: the commit list with its row menu,
/// paginating as the user reaches the end. No composer here — the Tauri
/// sidebar shows only the list on History, and amending is what sends the
/// user to Changes.
struct HistorySidebar: View {
    let commits: [CommitInfo]
    /// Drives the ↑ badge and gates the row menu's history-rewriting items:
    /// `headSha` says which row is `HEAD`, `unpushedShas` and `upstream` say
    /// whether the last commit is still safely local.
    let status: RepoStatus?

    /// Whether this repository's first `git log` has landed. An empty list
    /// says nothing on its own until it has (`RepoStore.historyLoaded`), so
    /// the placeholder below waits for it rather than asserting *no commits*
    /// over a read that is still in flight.
    let historyLoaded: Bool

    /// The highlighted commits, keyed by sha so they survive a log refresh that
    /// replaces every row value (same idea as the Changes tab's path
    /// selection). A **set**, because the history actions act on several commits
    /// at once; shift-click, ⌘-click, shift-arrow and ⌘A are AppKit's. Owned by
    /// the repository screen: a tab switch rebuilds this view, and the detail
    /// lives on the far side of the split.
    @Binding var selection: Set<String>

    /// The commit whose detail the main content shows. Derived from `selection`
    /// through `ListSelection`, which is the one place that rule lives: it
    /// follows a single-row selection and holds still for a multi-row one.
    @Binding var selectedSha: String?

    /// Ask the owner for another page when the list nears its last row.
    let onReachEnd: () -> Void

    /// The one answer to "may background work run right now?" — the date tick
    /// names its predicate here rather than composing its own visibility
    /// check, which is the whole point of the policy existing.
    let policy: BackgroundSchedulingPolicy

    /// Put the composer into amend mode for this commit and show it.
    let onAmend: (CommitInfo) -> Void
    /// Drop this commit, keeping its changes and message for a new one.
    let onUndo: (CommitInfo) -> Void
    /// Check the commit out, detaching HEAD. Answers with core's error text,
    /// or `nil` once HEAD is actually on the commit — the sheet stays up for
    /// the length of the call and keeps a refusal inside itself.
    let onCheckout: (CommitInfo) async -> String?
    /// Cherry-pick these commits — the menu's targets, newest first.
    let onCherryPick: ([CommitInfo]) -> Void
    /// Squash these commits into one — the menu's targets, newest first.
    let onSquash: ([CommitInfo]) -> Void

    /// A repository write is running, so the actions that replay commits
    /// cannot start. The operation in progress and a detached HEAD, which gate
    /// them too, are read off `status`.
    let isWriteInFlight: Bool

    /// The commit the checkout confirmation is about; `nil` when it's closed.
    @State private var commitToCheckout: CommitInfo?

    /// "Now", as the relative dates read it — bumped on a tick so the visible
    /// labels keep ageing. See `relativeDateClock`.
    @State private var now = Date.now

    private var unpushedShas: Set<String> { Set(status?.unpushedShas ?? []) }

    /// The last few shas, so scrolling *near* the end asks for the next page
    /// rather than scrolling *to* it. With the trigger on the final row the
    /// request only went out once the user had already run out of list, and
    /// the page landed under a scroller that had stopped; five rows of margin
    /// is the same overscan the Tauri virtualizer keeps.
    private var prefetchTriggerShas: Set<String> {
        Set(commits.suffix(Self.prefetchMargin).map(\.sha))
    }

    private static let prefetchMargin = 5

    var body: some View {
        Group {
            if !commits.isEmpty {
                commitList
            } else if historyLoaded {
                EmptyListPlaceholder(text: "No commits yet")
            } else {
                // Neither claim is safe yet, so the pane makes neither.
                EmptyListPlaceholder(text: "Loading history…")
            }
        }
        // Keep something selected: the newest commit on arrival, and again when
        // a refresh drops every selected sha — which is what an amend does.
        .maintainsSelection($selection, showing: $selectedSha, of: commits)
        .sheet(item: $commitToCheckout) { commit in
            CheckoutCommitSheet(commit: commit, isWriteInFlight: isWriteInFlight) {
                await onCheckout(commit)
            }
        }
    }

    private var commitList: some View {
        // Both sets are built once per body evaluation and captured by the row
        // closure. Read as computed properties they would be rebuilt inside the
        // closure — once per mounted row, on every repaint, including the 10 s
        // relative-date tick — which is thousands of string hashes for two
        // answers that do not vary across the rows they are asked about.
        let unpushed = unpushedShas
        let prefetchTriggers = prefetchTriggerShas

        // Restores the reader's place after a tab round trip, which takes this
        // whole subtree out of the hierarchy and rebuilds it scrolled to the
        // top. The anchor is the hoisted selection rather than a saved offset:
        // `selectedSha` already survives the trip, and a row id is a stable
        // thing to scroll back to where a pixel offset is not — the list can
        // have grown a page or lost the rewritten commit in between. The trade
        // is that it restores the *selection*, so a deep scroll made without
        // selecting anything still comes back at the top.
        return ScrollViewReader { proxy in
            List(commits, selection: $selection) { commit in
                CommitRow(
                    commit: commit,
                    isUnpushed: unpushed.contains(commit.sha),
                    now: now
                )
                .onAppear {
                    // Rows materialise lazily, so one of the last few
                    // appearing means the end of what we have is in sight.
                    if prefetchTriggers.contains(commit.sha) { onReachEnd() }
                }
            }
            .listStyle(.inset)
            .alternatingRowBackgrounds()
            .contextMenu(forSelectionType: String.self) { shas in
                // In list order, newest first — a set has no order, and every
                // history action names a *range*. Read through `targets`, never
                // off `selection`: the set AppKit hands over can lag behind a
                // selection made in code, and `targets` drops what the list no
                // longer holds.
                let targets = ListSelection.targets(shas, in: commits)
                if targets.count == 1, let commit = targets.first {
                    rowMenu(for: commit)
                } else if targets.count > 1 {
                    // Only what acts on all of them: every single-commit item
                    // would have to pick one row to mean.
                    cherryPickItem(for: targets)
                    squashItem(for: targets)
                }
            }
            .onAppear {
                // Not animated and not in `.task`: this is a restore, so it
                // should look like the list was never away.
                if let selectedSha { proxy.scrollTo(selectedSha) }
            }
            // A selection made in code — the commits a cherry-pick just landed
            // — can be anywhere relative to where the list is scrolled, and
            // AppKit only follows selections it made itself. With no anchor
            // this scrolls the least that reveals the row, so for a click or an
            // arrow key, whose row is already on screen, it does nothing.
            .onChange(of: selectedSha) { _, sha in
                if let sha { proxy.scrollTo(sha) }
            }
        }
        .task(id: policy.canTickRelativeDates) { await relativeDateClock() }
    }

    /// Re-render the visible rows' ages every 10 s, so an open History tab
    /// never goes stale (FRONTEND §6.12).
    ///
    /// Keyed on the policy's predicate rather than looping over it: a hidden
    /// window re-runs this with `false` and the task simply returns, so there
    /// is no timer left running to check a flag. Coming back re-keys it and
    /// starts a fresh one — which also bumps `now` immediately, so a window
    /// that was away for an hour is current the moment it is on screen again
    /// rather than up to 10 s later.
    ///
    /// Its cost is bounded by the list being lazy: only the mounted rows
    /// re-render, however deep the history has been paged.
    private func relativeDateClock() async {
        guard policy.canTickRelativeDates else { return }
        now = .now
        while !Task.isCancelled {
            // `try?` swallows the cancellation, so the loop condition above is
            // what actually ends this — see WS-P's finding on cancelled sleeps.
            try? await Task.sleep(for: .seconds(10))
            guard !Task.isCancelled else { return }
            now = .now
        }
    }

    // MARK: Row actions

    /// The right-clicked commit's menu. Amend and Undo only make sense on
    /// `HEAD`, and Checkout only on anything else, so exactly one of those two
    /// groups is ever live for a given row; Cherry-pick applies to any commit.
    /// What does not apply is shown disabled rather than hidden, so the menu
    /// keeps a stable shape.
    @ViewBuilder
    private func rowMenu(for commit: CommitInfo) -> some View {
        let isHead = commit.sha == status?.headSha

        Button("Amend Last Commit…") { onAmend(commit) }
            .disabled(!isHead)

        Button("Undo Last Commit") { onUndo(commit) }
            .disabled(!isHead || !canUndo(commit))

        Button("Check Out Commit…") { commitToCheckout = commit }
            .disabled(isHead)

        cherryPickItem(for: [commit])

        Divider()

        Button("Copy SHA") { Clipboard.copy(commit.sha) }
        Button("Copy Tag") { Clipboard.copy(commit.tags.joined(separator: " ")) }
            .disabled(commit.tags.isEmpty)
    }

    /// Cherry-pick, worded for how many commits it acts on. One item for both
    /// menus: a single commit is the same machinery with N = 1.
    private func cherryPickItem(for targets: [CommitInfo]) -> some View {
        Button(
            targets.count == 1 ? "Cherry-pick Commit…" : "Cherry-pick \(targets.count) Commits…"
        ) {
            onCherryPick(targets)
        }
        .disabled(!canStartHistoryAction)
    }

    /// Squash replays the branch from the oldest selected commit up, so beyond
    /// the shared gate it is off when a merge commit sits in that stretch —
    /// read off the rows, which are all loaded from HEAD down to any selected
    /// one. Disabled rather than hidden; a native menu item has no hover text,
    /// so the reason is core's to give to anyone who gets past this.
    private func squashItem(for targets: [CommitInfo]) -> some View {
        Button("Squash \(targets.count) Commits…") {
            onSquash(targets)
        }
        .disabled(
            !canStartHistoryAction
                || HistoryRange.replaysMergeCommit(in: commits, selected: Set(targets.map(\.sha)))
        )
    }

    /// The menu-time gate on every action that replays commits: not while an
    /// operation is in progress, not from a detached HEAD, and not under
    /// another write. Core's preflight stays the authority on everything that
    /// takes a git call to know — a dirty tree, the git floor.
    private var canStartHistoryAction: Bool {
        guard let status else { return false }
        return status.operation == nil && !status.detached && !isWriteInFlight
    }

    /// Undo is offered only while the commit is believed to be local: either
    /// it's provably unpushed, or no upstream resolved at all — in which case
    /// nothing can prove it *was* pushed either. Undoing a published commit
    /// would leave the branch behind its remote and needing a force push.
    private func canUndo(_ commit: CommitInfo) -> Bool {
        let hasResolvedUpstream = !(status?.upstream ?? "").isEmpty
        return !hasResolvedUpstream || unpushedShas.contains(commit.sha)
    }
}

/// One commit in the list: summary with tag chips and the unpushed badge,
/// then author · relative time — the Tauri list's two-line row.
private struct CommitRow: View {
    let commit: CommitInfo
    let isUnpushed: Bool
    /// The list's ticking clock, passed in rather than read here so every row
    /// ages against the same instant and one tick re-renders them together.
    let now: Date

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(commit.summary)
                    .lineLimit(1)

                Spacer(minLength: 0)

                // First tag plus an overflow count, like the Tauri list — a
                // narrow row can't afford a parade of chips.
                if let tag = commit.tags.first {
                    chip(tag)
                        .help(commit.tags.joined(separator: ", "))
                    if commit.tags.count > 1 {
                        chip("+\(commit.tags.count - 1)")
                            .help(commit.tags.joined(separator: ", "))
                    }
                }

                if isUnpushed {
                    // A 16×16 plate rather than a bare glyph — the Tauri
                    // list's unpushed badge, sharing the tag chips' fill and
                    // corner radius so the row's indicator cluster reads as
                    // one family.
                    Image(systemName: "arrow.up")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 16, height: 16)
                        .background(.quaternary, in: .rect(cornerRadius: 5))
                        .help("Not yet pushed")
                }
            }

            Text("\(commit.authorName) · \(CommitDate.relative(commit.authorDate, now: now))")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.vertical, 3)
        .help(CommitDate.absolute(commit.authorDate))
    }

    /// A tag's chip: STYLE.md's neutral badge, not an accent one.
    ///
    /// The accent it used to wear made a tag read as the row's most important
    /// thing and, worse, made it the *only* indicator with a colour — sitting
    /// a few points from the unpushed plate, which is the one that actually
    /// asks something of the user. Both are labels about the commit, so both
    /// take the same quaternary plate at the same radius.
    private func chip(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(.quaternary, in: .rect(cornerRadius: 5))
            .fixedSize()
    }
}

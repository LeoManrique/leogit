import SwiftUI

/// How a `RepoPickerList` is sized by whatever is showing it.
///
/// Not cosmetic. A container that fits its content measures this list *once*,
/// and what the list holds during that pass is the placeholder — `rows` is
/// state seeded by an `onChange(initial:)` that runs after it, and on a cold
/// open discovery has genuinely published nothing yet either. Fitted, a
/// popover freezes at placeholder height and the rows then arrive into a
/// three-row window. `fill` says the container has already decided its own
/// size, so nothing here needs measuring.
enum RepoPickerHeight {
    /// Grow with the rows, up to a ceiling — the Welcome card, which sits in a
    /// window that would otherwise let it run the full height of the screen.
    case upTo(CGFloat)

    /// Take whatever the header and footer leave.
    case fill

    /// The ceiling to hand `frame(maxHeight:)`.
    var limit: CGFloat {
        switch self {
        case .upTo(let limit): limit
        case .fill: .infinity
        }
    }

    /// Whether the placeholder stretches with the rows. Without it the footer
    /// floats up under a short placeholder in a fixed-size container, leaving
    /// the popover half empty above and below it.
    var placeholderMaxHeight: CGFloat? {
        switch self {
        case .upTo: nil
        case .fill: .infinity
        }
    }
}

/// The repository list, in the one form both places that show it use: the
/// Welcome screen's body and the toolbar switcher's popover.
///
/// The two are one component rather than a matched pair of copies because they
/// drifted before — the empty state, the footer and the search input set had
/// all diverged between the clients' two lists — and everything shown in both
/// is therefore shown by the same code. Only what genuinely differs is a
/// parameter: how the rows are sized, whether a repository is already open,
/// and whether something is currently holding switching back.
///
/// Rows are labelled by the remote's repository name where one is known
/// (`RepoIdentifierStore`), and the query searches every label a row displays —
/// the owner-qualified one included, which is what the shared `filter_repos`
/// rule exists to keep true in both clients.
struct RepoPickerList: View {
    /// The open repository, checkmarked and pinned to the top of both sort
    /// orders. `nil` on Welcome, where nothing is open yet.
    let activePath: String?

    let directory: RepoDirectoryStore
    let identifiers: RepoIdentifierStore

    /// Non-nil while switching repositories is held back — a transfer owns the
    /// single network slot, and swapping repositories under it would reset the
    /// sync UI from beneath the running operation. It disables the *rows*
    /// only: browsing the list and cloning contend with nothing, and cloning
    /// claims no slot in either client.
    let switchBlockedReason: String?

    /// How the scrolling rows are sized in the container showing them — the
    /// only geometry that differs between the two.
    let height: RepoPickerHeight

    let onSelect: (String) -> Void
    let onClone: () -> Void

    /// Opening Settings is the caller's, not this view's: the switcher has to
    /// dismiss its popover first, and a Settings window opening behind a
    /// popover that is still up is the bug that hides.
    let onChooseFolders: () -> Void

    @State private var filter = ""

    /// The rows on screen, and the labels among them that need an owner to tell
    /// apart — derived state, held rather than recomputed per body pass.
    ///
    /// Each pass would otherwise re-rank the whole list and, with a query
    /// typed, cross into core for the match; and the body re-runs once per
    /// label the identifier store publishes, so on a machine with fifty
    /// repositories that is fifty rankings and fifty crossings for one list
    /// appearing. `Inputs` names what the rows are actually a function of.
    @State private var rows: [String] = []
    @State private var collidingLabels: Set<String> = []

    /// The keyboard cursor, as a *path* rather than an index: the row set
    /// changes underneath it — a walk publishes, the sort flips, a query
    /// narrows — and an index would then point at whatever had moved into that
    /// slot. A cursor whose row is gone falls back to the first row.
    @State private var cursorPath: String?

    @FocusState private var filterFocused: Bool

    /// Everything `rows` is derived from. Equatable so the rebuild runs when
    /// one of them moves and not when anything else does.
    private struct Inputs: Equatable {
        var repos: [String]
        var recents: [String]
        var sortMode: SortMode
        var activePath: String?
        var query: String
        /// The identifier store's write count — cheaper to compare than the
        /// dictionary, and the labels feed both the order and the search.
        var labelRevision: Int
    }

    private var inputs: Inputs {
        Inputs(
            repos: directory.repos,
            recents: directory.recentRepos,
            sortMode: directory.sortMode,
            activePath: activePath,
            query: filter.trimmingCharacters(in: .whitespaces),
            labelRevision: identifiers.revision
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                TextField("Filter repositories", text: $filter)
                    .textFieldStyle(.roundedBorder)
                    .focused($filterFocused)
                    .onSubmit { openCursorRow() }
                    // The cursor moves while the *filter* keeps focus, which is
                    // what a `List(selection:)` cannot do: it only moves a
                    // cursor when the list itself is first responder, and
                    // taking focus away from the field would end the typing
                    // that produced these rows.
                    .onKeyPress(keys: [.upArrow, .downArrow]) { press in
                        moveCursor(by: press.key == .downArrow ? 1 : -1)
                        return .handled
                    }

                sortToggle
            }
            .padding(10)

            Divider()

            if let message = directory.discoveryError {
                discoveryFailure(message)
                Divider()
            }

            if rows.isEmpty {
                RepoListEmptyState(
                    // "Nothing found" is only news once something has looked:
                    // before the first pass finishes — which includes the whole
                    // of launch resolution, while this screen is already up —
                    // the honest answer is that the walk hasn't happened yet.
                    // Afterwards only an empty list can go back to looking, so
                    // a later refresh replaces rows in place rather than
                    // blinking the list through a spinner.
                    isDiscovering: !directory.hasSearched
                        || (directory.isRefreshing && directory.repos.isEmpty),
                    hasRepos: !directory.repos.isEmpty,
                    scannedPaths: directory.scanFolders,
                    onChooseFolders: onChooseFolders
                )
                .frame(maxHeight: height.placeholderMaxHeight)
            } else {
                rowList
            }

            Divider()

            // Getting a repository that isn't listed. There is deliberately no
            // "open this one folder" action beside it: the list is what the
            // scan paths cover, so a local repository missing from it means the
            // paths are wrong — a Settings edit that also holds next launch,
            // where a one-off open would not. The footer sits outside the rows
            // so it survives every empty state, which are the moments the user
            // most needs a way out.
            HStack {
                Spacer()
                Button("Clone Repository…", action: onClone)
            }
            .buttonStyle(.borderless)
            .padding(8)
        }
        .onAppear { filterFocused = true }
        .onChange(of: inputs, initial: true) { previous, current in
            rebuild(queryChanged: previous.query != current.query)
        }
        // Labels for every listed row, not just the visible ones: they are
        // searchable, so a query has to reach a repository the user has never
        // scrolled to. The store's own worker pool is what keeps that
        // affordable.
        .onChange(of: directory.repos, initial: true) { _, repos in
            identifiers.ensure(repos)
        }
    }

    // MARK: Rows

    private var rowList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(rows, id: \.self) { path in
                        let label = identifiers.label(of: path)
                        RepoListRow(
                            path: path,
                            label: label,
                            // GitHub Desktop's disambiguation rule: a row shows
                            // its owner only when another row shares its name,
                            // and only when there is an owner to show.
                            owner: collidingLabels.contains(label)
                                ? identifiers.identifier(of: path)?.owner : nil,
                            fullLabel: identifiers.fullLabel(of: path),
                            isActive: path == activePath,
                            isCursor: path == cursorPath,
                            sync: directory.syncByPath[path],
                            blockedReason: switchBlockedReason,
                            action: { onSelect(path) }
                        )
                        .id(path)
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(maxHeight: height.limit)
            // `anchor: nil` scrolls the least amount that reveals the row, so
            // an already-visible cursor never makes the list jump under a
            // mouse user — the Tauri lists' `block: 'nearest'`.
            .onChange(of: cursorPath) { _, path in
                guard let path else { return }
                proxy.scrollTo(path, anchor: nil)
            }
        }
    }

    private var sortToggle: some View {
        Button {
            directory.toggleSortMode()
        } label: {
            Image(systemName: directory.sortMode == .recent ? "clock" : "textformat.abc")
        }
        // The glyph is the state label, so the sentence lives in the tooltip —
        // "opened", not "modified": this list is ordered by when the user was
        // last *in* a repository, which is a different question from when a
        // commit last landed in it.
        .help(
            directory.sortMode == .recent
                ? "Sorted by recently opened" : "Sorted alphabetically"
        )
    }

    /// The walk failed. One inline row, not a phase swap: whatever a previous
    /// pass found is still listed below and still openable, and replacing the
    /// list with an error screen would take the repositories away along with
    /// the bad news.
    private func discoveryFailure(_ message: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
            Text("Couldn't search for repositories.")
            Spacer(minLength: 8)
            Button("Retry") {
                Task { await directory.refreshDirectory() }
            }
            .buttonStyle(.link)
            // A second click would coalesce into the running pass and look
            // like nothing happened; the disabled state is the feedback.
            .disabled(directory.isRefreshing)
        }
        .font(.caption)
        .help(message)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.orange.opacity(0.12))
    }

    // MARK: Ordering and search

    /// Re-derive the rows. Called from the one `onChange` that watches
    /// `Inputs`, so the ranking, the disambiguation set and the query all
    /// describe the same list and none of them runs on a body pass.
    private func rebuild(queryChanged: Bool) {
        // One label per repository per rebuild. Read straight from the store,
        // each of `label`/`fullLabel`/`searchLabels` rebuilds the folder name
        // from a `URL` — and the sort alone asks for two per comparison.
        let labels = Dictionary(
            uniqueKeysWithValues: directory.repos.map { ($0, identifiers.label(of: $0)) }
        )
        let ranked = ranked(directory.repos, labels: labels)
        collidingLabels = colliding(in: ranked, labels: labels)
        rows = matching(ranked)

        // A new query re-ranks the list, so the cursor belongs on the best
        // match — which is what makes Return act on what was just typed. Any
        // other rebuild keeps the row the cursor was on, if it survived.
        if queryChanged || cursorPath.map({ !rows.contains($0) }) ?? true {
            cursorPath = rows.first
        }
    }

    private func moveCursor(by delta: Int) {
        let current = cursorPath.flatMap { rows.firstIndex(of: $0) } ?? -1
        let next = ListNavigation.nextIndex(after: current, count: rows.count, delta: delta)
        cursorPath = rows[safe: next]
    }

    private func openCursorRow() {
        guard switchBlockedReason == nil, let path = cursorPath else { return }
        onSelect(path)
    }

    /// Unfiltered order: the open repository first, then — depending on the
    /// persisted toggle — most-recently-opened, or A-Z. The active row leads in
    /// *both* modes, because it is the one row whose position answers "where am
    /// I" rather than "what else is there".
    private func ranked(_ paths: [String], labels: [String: String]) -> [String] {
        let mru = Dictionary(
            directory.recentRepos.enumerated().map { ($0.element, $0.offset) },
            uniquingKeysWith: { first, _ in first }
        )
        let sortMode = directory.sortMode
        return paths.sorted { lhs, rhs in
            if (lhs == activePath) != (rhs == activePath) { return lhs == activePath }
            if sortMode == .recent {
                let left = mru[lhs] ?? Int.max
                let right = mru[rhs] ?? Int.max
                if left != right { return left < right }
            }
            switch NameCollation.compare(labels[lhs] ?? lhs, labels[rhs] ?? rhs) {
            case .orderedAscending: return true
            case .orderedDescending: return false
            // Two repositories really can share a label. The path is the
            // tiebreak that makes this a total order — without one, Swift's
            // unstable sort would let them swap places between passes.
            case .orderedSame: return lhs < rhs
            }
        }
    }

    /// Narrow `ranked` by the query. Core ranks by match quality and keeps the
    /// caller's order within a tier, so the ranking above survives filtering
    /// rather than being scrambled by it — which is what makes the first row
    /// the right thing for Return to act on.
    private func matching(_ ranked: [String]) -> [String] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return ranked }
        return GitBridge.matchingRepos(
            query: query,
            rows: ranked.map { RepoRow(path: $0, names: identifiers.searchLabels(of: $0)) },
            scanFolders: directory.scanFolders
        )
    }

    /// Labels more than one row would show. Computed over the whole list, not
    /// the filtered one, so a row's owner prefix doesn't appear and disappear
    /// as the user types.
    private func colliding(in paths: [String], labels: [String: String]) -> Set<String> {
        var seen: Set<String> = []
        var colliding: Set<String> = []
        for path in paths {
            guard let label = labels[path] else { continue }
            if !seen.insert(label).inserted { colliding.insert(label) }
        }
        return colliding
    }
}

/// One repository row: checkmark slot, label, and the three indicators.
private struct RepoListRow: View {
    let path: String
    let label: String

    /// The owning account, rendered as a muted `owner/` prefix — present only
    /// where the label alone would be ambiguous.
    let owner: String?

    let fullLabel: String
    let isActive: Bool

    /// Whether the keyboard cursor is on this row. Distinct from `isActive`,
    /// which marks the repository that is already open: one says "this is
    /// where you are", the other "this is what Return will do".
    let isCursor: Bool

    let sync: RepoSync?
    let blockedReason: String?
    let action: () -> Void

    @State private var isHovered = false

    private var isBlocked: Bool { blockedReason != nil }

    var body: some View {
        Button {
            guard !isBlocked else { return }
            action()
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                    .opacity(isActive ? 1 : 0)
                    .frame(width: 14)

                name

                Spacer(minLength: 12)

                if let sync {
                    indicators(for: sync)
                }
            }
            .lineLimit(1)
            .truncationMode(.middle)
            .contentShape(.rect)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
        }
        .buttonStyle(.plain)
        // Dimmed and inert rather than `.disabled`, deliberately: a disabled
        // control takes no pointer events, so the tooltip explaining *why* the
        // row can't be picked would never appear — which is the only reason
        // the row is still on screen rather than hidden.
        .opacity(isBlocked ? 0.55 : 1)
        .background(rowFill)
        .onHover { isHovered = $0 }
        .help(blockedReason ?? tooltip)
    }

    /// The row's name, with the owner ahead of it as one run of text rather
    /// than a second view: they are one label, and an `HStack` would let the
    /// truncation fall between them and ellipsize the owner while leaving the
    /// name whole.
    private var name: Text {
        guard let owner else { return Text(verbatim: label) }
        var prefix = AttributedString("\(owner)/")
        prefix.foregroundColor = .secondary
        return Text(prefix + AttributedString(label))
    }

    /// The cursor reads as a selection; hover stays the lighter wash, so the
    /// two can be told apart when the pointer is resting on a different row.
    private var rowFill: AnyShapeStyle {
        guard !isBlocked else { return AnyShapeStyle(.clear) }
        if isCursor { return AnyShapeStyle(.selection.opacity(0.35)) }
        if isHovered { return AnyShapeStyle(.selection.opacity(0.15)) }
        return AnyShapeStyle(.clear)
    }

    /// The owner-qualified name over the full path — the path alone leaves a
    /// row whose label came from its remote unexplained.
    private var tooltip: String {
        fullLabel == label ? path : "\(fullLabel)\n\(path)"
    }

    /// Dirty dot, then ↓ pending pulls, then ↑ pending pushes — the same
    /// three badges as the Tauri dropdown, with its tooltip wording.
    @ViewBuilder
    private func indicators(for sync: RepoSync) -> some View {
        HStack(spacing: 6) {
            if sync.dirty {
                Circle()
                    .fill(.tint)
                    .frame(width: 6, height: 6)
                    .help("Uncommitted changes")
            }
            if sync.hasRemote, sync.behind > 0 {
                Text("↓\(sync.behind)")
                    .help("\(sync.behind) commit(s) to pull")
            }
            if sync.hasRemote, sync.ahead > 0 {
                Text("↑\(sync.ahead)")
                    .help("\(sync.ahead) commit(s) to push")
            }
        }
        .font(.caption.monospacedDigit())
        .foregroundStyle(.secondary)
    }
}

extension Array {
    /// The element at `index`, or `nil` when it is out of bounds — the cursor
    /// can point past a list the query has just shortened.
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

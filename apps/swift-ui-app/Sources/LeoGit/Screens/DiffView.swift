import SwiftUI

/// The diff for one selected file: a header naming the file, its +/− totals
/// and the layout control, then the change itself — one column of lines, or
/// the old and new sides beside each other. Serves both the Changes tab
/// (working tree) and the History detail (a commit) — `target` says which.
struct DiffView: View {
    let repoPath: String
    let file: FileEntry
    let target: DiffTarget

    @State private var store = DiffStore()
    @Environment(\.colorScheme) private var colorScheme
    @Environment(AppConfigStore.self) private var appConfig

    /// Reload whenever the selection or the source changes — a new file, a
    /// status epoch bump, a different commit — or a setting the *read* depends
    /// on: toggling any of them re-runs the load through the seamless path
    /// (the Tauri client's `diffReadKey` effect), where the equality skip
    /// keeps scroll and colour when nothing textual changed. The layout
    /// is one of them because core builds the row pairing only for the
    /// arrangement about to render it. Tab size is pure presentation — it
    /// re-renders without reloading, so it stays out.
    private struct LoadKey: Equatable {
        let path: String
        let target: DiffTarget
        let hideWhitespace: Bool
        let highlight: Bool
        let sideBySide: Bool
    }

    /// Whitespace hiding applies to working-tree diffs only — core has no
    /// whitespace-ignored commit read, and the Tauri client fetches commit
    /// diffs the same way — so a commit target never re-keys on the toggle.
    private var hideWhitespace: Bool {
        if case .workingTree = target { appConfig.hideWhitespace } else { false }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            // Every branch of `content` must claim the pane. An empty state
            // sized to its own ideal height leaves the whole stack shorter
            // than the split slot, which then centres it — pushing the header
            // into the middle of the pane, where it reads as an oversized
            // header rather than as a short body.
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                // Crossing the slow threshold dims what is on screen and lays
                // a spinner over it; it never unmounts it. Swapping the pane
                // for a bare `ProgressView` gave SwiftUI a different view to
                // build, so the `ScrollView` was destroyed and rebuilt at the
                // top — undoing, in the view, exactly what the store's
                // equality skip exists to preserve. The content is still the
                // truth about the file the user is reading; the dim says a
                // newer answer is on its way.
                .opacity(isReloadingSlowly ? 0.45 : 1)
                .animation(.easeOut(duration: 0.12), value: isReloadingSlowly)
                .overlay(alignment: .top) {
                    if isReloadingSlowly {
                        // Near the top rather than centred: the rows
                        // underneath are what is being read, and the pane can
                        // be tall enough that its middle is below the fold.
                        ProgressView()
                            .controlSize(.small)
                            .padding(.top, 48)
                            .allowsHitTesting(false)
                    }
                }
        }
        .task(
            id: LoadKey(
                path: file.path,
                target: target,
                hideWhitespace: hideWhitespace,
                highlight: appConfig.syntaxHighlighting,
                sideBySide: appConfig.sideBySideDiff
            )
        ) {
            // A dirty submodule has no diff worth reading — see `content`.
            guard !isDirtySubmodule else { return }
            await store.load(
                repoPath: repoPath,
                file: file,
                target: target,
                hideWhitespace: hideWhitespace,
                highlight: appConfig.syntaxHighlighting,
                sideBySide: appConfig.sideBySideDiff
            )
        }
    }

    private var isReloadingSlowly: Bool {
        store.phase == .loading(slow: true)
    }

    /// A submodule changed *inside* while the commit the parent records has
    /// not moved. `git diff` answers with a bare `Subproject commit <sha>-dirty`
    /// line, which is not a diff of anything the user can act on from here —
    /// so the pane explains instead, and the read is never made. Because that
    /// read never happens, the store still holds whichever file was open
    /// before, which is why the header's own chrome asks this too.
    private var isDirtySubmodule: Bool {
        if case .workingTree = target { file.submoduleDirty } else { false }
    }

    private var header: some View {
        HStack(spacing: 10) {
            FileStatusBadge(file: file)

            VStack(alignment: .leading, spacing: 1) {
                Text(file.displayName)
                    .font(.headline)
                if !file.displayDir.isEmpty {
                    Text(file.displayDir)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer(minLength: 8)

            if let payload = store.payload, !isDirtySubmodule {
                HStack(spacing: 6) {
                    Text("+\(payload.additions)")
                        .foregroundStyle(palette.addGlyph)
                    Text("−\(payload.deletions)")
                        .foregroundStyle(palette.removeGlyph)
                }
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
            }

            if showsRows {
                layoutPicker
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    /// Whether there are diff rows on screen for the header to describe — a
    /// binary stand-in, an empty state or a failure has nothing to arrange,
    /// and offering a layout control over one would be a control that does
    /// nothing.
    private var showsRows: Bool {
        !isDirtySubmodule && store.payload?.fileDiff.isBinary == false
    }

    /// Unified or split, in the diff's own header rather than in Settings:
    /// GitHub Desktop treats the arrangement as a property of the diff you are
    /// reading, and a reader who wants the other one wants it *here*, not
    /// after a detour through a settings window. It still persists — into the
    /// same shared `side_by_side_diff` both clients read — so the choice
    /// outlives the file, the repository and the app.
    private var layoutPicker: some View {
        Picker("Diff layout", selection: layoutBinding) {
            Image(systemName: "rectangle.grid.1x2")
                .accessibilityLabel("Unified")
                .tag(false)
            Image(systemName: "rectangle.split.2x1")
                .accessibilityLabel("Split")
                .tag(true)
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .fixedSize()
        .help("Show the change as one column or as old and new side by side")
    }

    /// Writes straight through to the shared config, which is what re-keys the
    /// load task above: core builds the pairing for the layout that asked, so
    /// the arrangement and the data behind it can never disagree.
    private var layoutBinding: Binding<Bool> {
        Binding {
            appConfig.sideBySideDiff
        } set: { sideBySide in
            appConfig.setSideBySideDiff(sideBySide)
        }
    }

    /// Seamless-switching rule (the Tauri/GH-Desktop contract, carried by
    /// `DiffStore.phase`): whatever was last shown — diff rows, the binary or
    /// empty notice — stays on screen while a reload runs. Nothing here
    /// branches on the slow threshold; that is a dim and an overlay in `body`,
    /// laid over whichever of these states is already up. A fast first load,
    /// with nothing old to keep showing, stays blank rather than flashing a
    /// sub-threshold spinner.
    @ViewBuilder
    private var content: some View {
        if isDirtySubmodule {
            ContentUnavailableView(
                "Submodule Changes",
                systemImage: "arrow.turn.down.right",
                description: Text(
                    """
                    This submodule has modified content that hasn't been committed. \
                    Those changes must be committed inside the submodule before they \
                    can be part of this repository.
                    """
                )
            )
        } else if case .failed(let message) = store.phase {
            ContentUnavailableView(
                "Couldn't Load Diff",
                systemImage: "exclamationmark.triangle",
                description: Text(message)
            )
        } else if store.payload?.fileDiff.isBinary == true {
            ContentUnavailableView(
                "Binary File",
                systemImage: "doc.zipper",
                description: Text("This change has no line-by-line diff.")
            )
        } else if let guardInfo = store.sizeGuard {
            // Withheld, never refused — without the button this state would
            // make a file with one long line permanently unreadable.
            ContentUnavailableView {
                Label("Large Diff", systemImage: "doc.text.magnifyingglass")
            } description: {
                Text(Self.sizeGuardExplanation(guardInfo))
            } actions: {
                Button("Show Diff Anyway") {
                    Task {
                        await store.loadIgnoringSizeGuard(
                            repoPath: repoPath,
                            file: file,
                            target: target,
                            hideWhitespace: hideWhitespace,
                            highlight: appConfig.syntaxHighlighting,
                            sideBySide: appConfig.sideBySideDiff
                        )
                    }
                }
            }
        } else if let reason = store.emptyReason {
            ContentUnavailableView(
                Self.emptyTitle(reason),
                systemImage: "doc",
                description: Text(Self.emptyExplanation(reason))
            )
        } else if store.payload == nil {
            Color.clear
        } else {
            diffBody
        }
    }

    /// The change itself. Long lines always wrap — the GitHub Desktop model,
    /// shared by both clients — so there is vertical scrolling only, in both
    /// arrangements.
    ///
    /// The two arrangements are branches *inside* one `ScrollView`, never two
    /// scroll views: swapping the whole pane would destroy the scroller and
    /// drop the reader at the top of the file every time they changed the
    /// layout, which is the same identity mistake the slow-load dim exists to
    /// avoid. And the branch reads the loaded *pairing*, not the setting — the
    /// pairing is what the split rows are built from, so the arrangement
    /// changes on the frame its data arrives rather than blanking the pane for
    /// the length of a re-read.
    private var diffBody: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if store.pairs.isEmpty {
                    unifiedRows
                } else {
                    splitRows
                }
            }
            .padding(.vertical, 4)
            .textSelection(.enabled)
        }
    }

    /// One row per diff line, in file order.
    private var unifiedRows: some View {
        ForEach(store.rows) { row in
            switch row.line.lineType {
            case .hunk:
                DiffHunkBand(text: row.line.text ?? row.line.content)
            case .noNewline:
                Text(row.line.text ?? row.line.content)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 1)
            case .context, .add, .delete:
                DiffLineCell(
                    line: row.line,
                    tokens: tokens(at: row.id),
                    gutter: .both(old: row.line.oldLineNo, new: row.line.newLineNo),
                    palette: palette,
                    tabSize: appConfig.tabSize
                )
            }
        }
    }

    /// One row per pairing: the old side on the left, the new on the right,
    /// with a filler cell wherever a side has no counterpart. A `@@` header
    /// describes the whole hunk, so it spans both columns instead.
    ///
    /// `NoNewline` markers have no row here at all — core leaves them out of
    /// the pairing, since "no newline at end of file" belongs to one side and
    /// would have to be invented for the other.
    private var splitRows: some View {
        ForEach(store.pairs) { pair in
            if pair.isHunkHeader, let line = line(at: pair.left) {
                DiffHunkBand(text: line.text ?? line.content)
            } else {
                HStack(alignment: .top, spacing: 0) {
                    splitCell(at: pair.left, showing: .old)
                    // The one rule between the panes (STYLE.md), stretched by
                    // the `HStack` so it runs the height of the taller side.
                    Divider()
                    splitCell(at: pair.right, showing: .new)
                }
            }
        }
    }

    /// Which of a line's two numbers a split column shows.
    private enum Column {
        case old
        case new
    }

    /// One column of a split row. A `nil` index is the filler cell: the change
    /// on the other side has no counterpart on this one.
    private func splitCell(at index: Int?, showing column: Column) -> some View {
        let line = line(at: index)
        return DiffLineCell(
            line: line,
            tokens: index.map(tokens(at:)) ?? [],
            gutter: .one(column == .old ? line?.oldLineNo : line?.newLineNo),
            palette: palette,
            tabSize: appConfig.tabSize
        )
    }

    /// The line at a flat index — the one row model both arrangements read, so
    /// a pairing and the rows it points into can never describe different
    /// text. `nil` for a filler cell, and for the index of a pairing that has
    /// briefly outlived the rows it was built against.
    private func line(at index: Int?) -> DiffLine? {
        guard let index, store.rows.indices.contains(index) else { return nil }
        return store.rows[index].line
    }

    private var palette: DiffPalette {
        DiffPalette(colorScheme)
    }

    /// Each empty state names what actually happened. They are three different
    /// situations, and one caption covering all of them told the user the file
    /// was unchanged when the whitespace setting was simply hiding the change.
    private static func emptyTitle(_ reason: EmptyDiffReason) -> String {
        switch reason {
        case .noChanges: "No Changes"
        case .whitespaceOnly: "Whitespace Only"
        case .noTextualChanges: "No Textual Changes"
        }
    }

    private static func emptyExplanation(_ reason: EmptyDiffReason) -> String {
        switch reason {
        case .noChanges:
            "This file matches its committed state."
        case .whitespaceOnly:
            "Every change here is whitespace, and Settings is set to hide those."
        case .noTextualChanges:
            "The file changed without changing any lines — a mode change or rename, for example."
        }
    }

    private static func sizeGuardExplanation(_ info: DiffSizeGuard) -> String {
        let megabytes = Double(info.bytes) / 1_048_576
        return switch info.reason {
        case .totalBytes:
            String(format: "This diff is %.1f MB — large enough to be slow to render.", megabytes)
        case .lineLength:
            "This diff has a line of \(info.longestLine) characters — long enough to be slow to render."
        }
    }

    /// The row's token line once phase two has landed; empty means "render
    /// plain", which is also the correct state while tokens are in flight.
    /// Keyed by flat index, which is what a split cell carries — so each side
    /// of a pair colours from its own line with no bookkeeping.
    private func tokens(at index: Int) -> [Token] {
        guard let tokens = store.tokens, index < tokens.count else { return [] }
        return tokens[index]
    }
}

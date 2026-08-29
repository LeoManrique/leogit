import AppKit
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

    /// Where the diff is scrolled. Bound rather than read: the pane has one
    /// scroller for every arrangement and every reload, so the only way to put
    /// a newly opened file at its first line is to say so.
    @State private var scrollPosition = ScrollPosition()

    /// Whether the diff pane holds focus. Copy is dispatched down the responder
    /// chain, so a pane that never takes focus is never offered the command.
    @FocusState private var isFocused: Bool

    @Environment(\.colorScheme) private var colorScheme
    @Environment(AppConfigStore.self) private var appConfig

    /// Everything the diff on screen is a function of, so a read happens when
    /// one of them moves and never otherwise.
    ///
    /// Three groups. **Which diff**: the path and the target. **Whether that
    /// diff is still what was read** — the file's own bytes (`statStamp`,
    /// core's mtime-and-size stamp, since porcelain v2 carries no worktree
    /// hash and a file that was modified and is still modified reads
    /// identically from one tick to the next), which side of it is being
    /// compared (`xy`, so staging or unstaging re-reads it), and the commit it
    /// is compared *against*, which rides in the target. **How it is read**:
    /// the whitespace, highlight and layout settings, each of which changes
    /// what core is asked for — the layout because the row pairing is built
    /// only for the arrangement about to render it.
    ///
    /// Keyed to this one file, not to the working tree: an unrelated edit
    /// elsewhere used to re-read and re-tokenize whatever was open, for an
    /// answer that could not have changed. Tab size is pure presentation — it
    /// re-renders without reloading, so it stays out.
    private struct LoadKey: Equatable {
        let path: String
        let xy: String
        let statStamp: String?
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
                xy: file.xy,
                statStamp: file.statStamp,
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
        // A *different* diff starts at its first line; the same diff re-read
        // keeps the reader where they were (FRONTEND §6.3). `rendered` is what
        // separates the two — it moves when the pane changes which diff it is
        // showing and holds still through every re-read of that diff, so a
        // layout toggle, a whitespace change, an edit on disk and a `HEAD` move
        // all leave the offset alone. Keyed on the pane's own scroller rather
        // than on the selection, so the jump happens as the new rows land
        // instead of throwing the old ones to the top first.
        //
        // Outside `content`, not inside it: a binary file or an empty state
        // takes the scroller out of the hierarchy, and a modifier that is not
        // there cannot notice the diff that replaces it.
        .onChange(of: store.rendered) {
            scrollPosition.scrollTo(edge: .top)
        }
        // ⌘C, and Edit ▸ Copy with it. `nil` when no line is picked out in
        // the gutter, and that `nil` is the whole arrangement: the pane
        // declines the command and it carries on down the responder chain to
        // the within-line text selection, which answers it as it always did.
        //
        // While a run *does* exist the run wins, because it is drawn as a
        // full-row wash and a visible selection that Copy ignored would be
        // worse than one that takes precedence. Escape is how it is given up.
        .onCopyCommand(perform: copyPayload)
        .onExitCommand { store.clearLineSelection() }
    }

    /// What a Copy puts on the pasteboard: the file's own lines for the
    /// selected run, or nothing to say, so the command falls through.
    ///
    /// Whether there is a run decides the *offer*; the text itself is built
    /// inside the closure, when a Copy actually happens. Building it here would
    /// join every selected line on every repaint — and "select all" over a
    /// twenty-thousand-line diff is exactly when repaints are least affordable.
    private var copyPayload: (() -> [NSItemProvider])? {
        guard store.lineSelection != nil else { return nil }
        return { [NSItemProvider(object: (store.selectedLineText ?? "") as NSString)] }
    }

    /// The gutter's actions, built once per render and handed to every cell
    /// rather than closed over per row.
    private var gutterActions: DiffGutterActions {
        DiffGutterActions(
            select: { index, extending in
                store.selectLine(index, extending: extending)
                // Copy is a responder-chain command, so the pane has to be the
                // thing holding focus for `onCopyCommand` to be asked at all.
                isFocused = true
            },
            copy: { index in
                // A right-click outside the run acts on the row under the
                // pointer, and makes it the run — so what was copied is what
                // stays highlighted.
                if store.lineSelection?.contains(index) != true {
                    store.selectLine(index, extending: false)
                }
                copyToPasteboard()
            },
            selectAll: {
                store.selectAllLines()
                isFocused = true
            }
        )
    }

    private func copyToPasteboard() {
        guard let text = store.selectedLineText else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
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
                subtitle
            }

            Spacer(minLength: 8)

            lineCounts

            if showsRows {
                layoutPicker
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    /// Where the file is — or, for a rename, where it *was* and where it is
    /// now, since rendering only the destination makes a rename read as an add
    /// and loses the one fact that matters about it. Both paths truncate from
    /// the head, so each keeps the filename that identifies it, the rule
    /// `PathText` applies in the file lists.
    @ViewBuilder
    private var subtitle: some View {
        if let renamedFrom {
            HStack(spacing: 4) {
                Text(renamedFrom)
                    .truncationMode(.head)
                Text("→")
                Text(file.path)
                    .truncationMode(.head)
            }
            .lineLimit(1)
            .font(.caption)
            .foregroundStyle(.secondary)
        } else if !file.displayDir.isEmpty {
            Text(file.displayDir)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    /// The `+N −N` badge, each side only when it has something to report.
    ///
    /// A file that only adds lines says `+42` and not `+42 −0`, and a diff with
    /// no lines at all — a binary stand-in, which core counts as zero on both
    /// sides — says nothing rather than `+0 −0`, which described the file as
    /// unchanged when it was a change the pane simply cannot draw.
    @ViewBuilder
    private var lineCounts: some View {
        if let payload = describedPayload, payload.additions > 0 || payload.deletions > 0 {
            HStack(spacing: 6) {
                if payload.additions > 0 {
                    Text("+\(payload.additions)")
                        .foregroundStyle(palette.addGlyph)
                }
                if payload.deletions > 0 {
                    Text("−\(payload.deletions)")
                        .foregroundStyle(palette.removeGlyph)
                }
            }
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
        }
    }

    /// The rendered diff, but only once it is a diff *of this file*.
    ///
    /// A reload is seamless, so between the click and the answer the payload on
    /// screen still describes the file that was open before — and the name
    /// beside it in this header does not. Everything the header derives from
    /// the payload comes through here, so it can never end up captioning one
    /// file with another's facts. A dirty submodule is the same situation held
    /// open indefinitely: no read is ever made for it, so the payload stays
    /// whatever was last looked at.
    private var describedPayload: DiffPayload? {
        guard !isDirtySubmodule, store.rendered == DiffIdentity(file, from: target) else {
            return nil
        }
        return store.payload
    }

    /// The path this file was renamed from, when it was renamed.
    ///
    /// From the status entry, not the parsed diff, and the reason is that the
    /// diff cannot answer it: both reads pathspec-limit to the file's *current*
    /// path (`git diff HEAD -- <path>`), so git never sees the deleted
    /// counterpart and reports a rename as a plain add. Its `--- a/…` and
    /// `+++ b/…` pair describes the same path twice, or `/dev/null` on the side
    /// that does not exist. `FileEntry.origPath` is filled by core from
    /// porcelain v2 in the working tree and from `--raw` in commit detail, and
    /// deliberately left empty for a copy, which has a source but took nothing
    /// from it. It is also the entry the header's own name comes from, so the
    /// two halves of that line can never describe different files — and it is
    /// what the changed-file lists already read.
    private var renamedFrom: String? {
        guard let origPath = file.origPath, origPath != file.path else { return nil }
        return origPath
    }

    /// Whether there are diff rows on screen for the layout control to act on —
    /// a binary stand-in, an empty state or a failure has nothing to arrange,
    /// and offering a control over one would be a control that does nothing.
    ///
    /// The rendered payload, not `describedPayload`: the control arranges what
    /// is *on screen*, which during a switch is still the previous file's diff
    /// and is still arrangeable. Gating it on the file being named instead
    /// would take the control away for the length of every load.
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
            // Granted to the stack so every line's `Text` is selectable. It
            // does *not* make one drag span lines: a SwiftUI selection cannot
            // leave the `Text` it began in, whatever the grant is attached to
            // (**D-22**). So this buys character selection inside a line, and
            // picking out several lines is the gutter run's job instead.
            .textSelection(.enabled)
        }
        .scrollPosition($scrollPosition)
        .focusable()
        .focusEffectDisabled()
        .focused($isFocused)
    }

    /// One row per diff line, in file order.
    private var unifiedRows: some View {
        ForEach(store.rows) { row in
            switch row.line.lineType {
            case .hunk:
                DiffHunkBand(
                    text: row.line.text ?? row.line.content,
                    isSelected: isSelected(row.id),
                    palette: palette
                )
            case .noNewline:
                Text(row.line.text ?? row.line.content)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(isSelected(row.id) ? palette.lineSelection : .clear)
            case .context, .add, .delete:
                DiffLineCell(
                    line: row.line,
                    tokens: tokens(at: row.id),
                    gutter: .both(old: row.line.oldLineNo, new: row.line.newLineNo),
                    rowIndex: row.id,
                    selection: store.lineSelection,
                    palette: palette,
                    tabSize: appConfig.tabSize,
                    actions: gutterActions
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
                DiffHunkBand(
                    text: line.text ?? line.content,
                    isSelected: pair.left.map(isSelected) ?? false,
                    palette: palette
                )
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
            // A filler cell stands for no row, so its gutter selects nothing —
            // which is also why the two columns can share one flat run: each
            // side offers only the indices it actually shows.
            rowIndex: line == nil ? nil : index,
            selection: store.lineSelection,
            palette: palette,
            tabSize: appConfig.tabSize,
            actions: gutterActions
        )
    }

    /// Whether a flat row is inside the reader's run — asked by the two row
    /// kinds that have no gutter of their own to answer it.
    private func isSelected(_ index: Int) -> Bool {
        store.lineSelection?.contains(index) ?? false
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

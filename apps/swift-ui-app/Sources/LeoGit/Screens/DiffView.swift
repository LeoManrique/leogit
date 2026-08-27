import SwiftUI

/// The diff for one selected file: a header naming the file and its +/−
/// totals, then one row per diff line. Serves both the Changes tab (working
/// tree) and the History detail (a commit) — `target` says which.
struct DiffView: View {
    let repoPath: String
    let file: FileEntry
    let target: DiffTarget

    @State private var store = DiffStore()
    @Environment(\.colorScheme) private var colorScheme
    @Environment(AppConfigStore.self) private var appConfig

    /// Reload whenever the selection or the source changes — a new file, a
    /// status epoch bump, a different commit — or a diff *content* setting:
    /// toggling either one re-runs the load through the seamless path (the
    /// Tauri client's `lastHideWhitespace` effect), where the equality skip
    /// keeps scroll when nothing textual changed. Tab size is presentation —
    /// it re-renders without reloading, so it stays out.
    private struct LoadKey: Equatable {
        let path: String
        let target: DiffTarget
        let hideWhitespace: Bool
        let highlight: Bool
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
        }
        .task(
            id: LoadKey(
                path: file.path,
                target: target,
                hideWhitespace: hideWhitespace,
                highlight: appConfig.syntaxHighlighting
            )
        ) {
            await store.load(
                repoPath: repoPath,
                file: file,
                target: target,
                hideWhitespace: hideWhitespace,
                highlight: appConfig.syntaxHighlighting
            )
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            FileStatusBadge(status: file.status)

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

            if let payload = store.payload {
                HStack(spacing: 6) {
                    Text("+\(payload.additions)")
                        .foregroundStyle(palette.addGlyph)
                    Text("−\(payload.deletions)")
                        .foregroundStyle(palette.removeGlyph)
                }
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    /// Seamless-switching rule (the Tauri/GH-Desktop contract, carried by
    /// `DiffStore.phase`): whatever was last shown — diff rows, the binary or
    /// empty notice — stays on screen while a reload runs, and the spinner
    /// appears only once the load outlives the slow threshold. A fast first
    /// load, with nothing old to keep showing, stays blank rather than
    /// flashing a sub-threshold spinner.
    @ViewBuilder
    private var content: some View {
        if case .failed(let message) = store.phase {
            ContentUnavailableView(
                "Couldn't Load Diff",
                systemImage: "exclamationmark.triangle",
                description: Text(message)
            )
        } else if store.phase == .loading(slow: true) {
            ProgressView()
        } else if store.payload?.fileDiff.isBinary == true {
            ContentUnavailableView(
                "Binary File",
                systemImage: "doc.zipper",
                description: Text("This change has no line-by-line diff.")
            )
        } else if store.isEmpty || (store.payload != nil && store.rows.isEmpty) {
            ContentUnavailableView(
                "No Textual Changes",
                systemImage: "doc",
                description: Text("The file changed without changing any lines — a mode change or rename, for example.")
            )
        } else if store.payload == nil {
            Color.clear
        } else {
            diffRows
        }
    }

    /// Long lines always wrap — the GitHub Desktop model, shared by both
    /// clients. Vertical scrolling only.
    private var diffRows: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(store.rows) { row in
                    DiffRowView(
                        row: row,
                        tokens: tokens(for: row),
                        palette: palette,
                        tabSize: appConfig.tabSize
                    )
                }
            }
            .padding(.vertical, 4)
            .textSelection(.enabled)
        }
    }

    private var palette: DiffPalette {
        DiffPalette(colorScheme)
    }

    /// The row's token line once phase two has landed; empty means "render
    /// plain", which is also the correct state while tokens are in flight.
    private func tokens(for row: DiffRow) -> [Token] {
        guard let tokens = store.tokens, row.id < tokens.count else { return [] }
        return tokens[row.id]
    }
}

/// One line of the diff: gutter line numbers, the change glyph, and the
/// line's content — syntax-coloured once tokens arrive.
private struct DiffRowView: View {
    let row: DiffRow
    let tokens: [Token]
    let palette: DiffPalette
    /// Tab stop width in columns, from the shared `tab_size` setting.
    let tabSize: Int

    var body: some View {
        switch row.line.lineType {
        case .hunk:
            // The `@@ -a,b +c,d @@ context` separator row.
            Text(row.line.text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 12)
                .padding(.vertical, 5)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.quaternary.opacity(0.5))
        case .noNewline:
            Text(row.line.text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 12)
                .padding(.vertical, 1)
        case .context, .add, .delete:
            HStack(alignment: .top, spacing: 0) {
                lineNumber(row.line.oldLineNo)
                lineNumber(row.line.newLineNo)
                Text(glyph)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(glyphColor)
                    .frame(width: 16)
                Text(attributedContent)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.trailing, 12)
            }
            .padding(.vertical, 1)
            .background(rowBackground)
        }
    }

    private func lineNumber(_ number: Int32?) -> some View {
        Text(number.map(String.init) ?? "")
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.quaternary)
            .frame(width: 40, alignment: .trailing)
            .padding(.trailing, 4)
    }

    private var glyph: String {
        switch row.line.lineType {
        case .add: "+"
        case .delete: "−"
        default: ""
        }
    }

    private var glyphColor: Color {
        switch row.line.lineType {
        case .add: palette.addGlyph
        case .delete: palette.removeGlyph
        default: .clear
        }
    }

    private var rowBackground: Color {
        switch row.line.lineType {
        case .add: palette.addRowBackground
        case .delete: palette.removeRowBackground
        default: .clear
        }
    }

    private var attributedContent: AttributedString {
        DiffLineText.attributed(
            content: row.line.content,
            tokens: tokens,
            intra: row.line.intraLineDiff,
            lineType: row.line.lineType,
            palette: palette,
            tabSize: tabSize
        )
    }
}

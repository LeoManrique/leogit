import SwiftUI

/// A run of diff lines the reader has picked out, as flat row indices.
///
/// The unit a diff is copied in. It is deliberately *not* the text selection
/// SwiftUI gives a `Text`: that one is a property of what was drawn, so what
/// it yields depends on where the drag happened to start and it carries the
/// viewer's own tab expansion. This is a range into the row model, which is
/// what `copy_diff_text` takes, so the copy is the file's own lines by
/// construction — in either arrangement, and whichever column was clicked.
///
/// Anchor and focus rather than a bare range, because a shift-click extends
/// from where the run *started*: clicking 10 then shift-clicking 4 selects
/// 4…10, and shift-clicking 20 after that selects 10…20, not 4…20.
struct DiffLineSelection: Equatable {
    private var anchor: Int
    private var focus: Int

    /// One line, and the anchor every later extension measures from.
    init(_ index: Int) {
        anchor = index
        focus = index
    }

    /// Extend the run to `index`, keeping the anchor where it was.
    mutating func extend(to index: Int) {
        focus = index
    }

    /// `[start, end)` over the flat row indices — `copy_diff_text`'s own range.
    var range: Range<Int> {
        Swift.min(anchor, focus)..<(Swift.max(anchor, focus) + 1)
    }

    func contains(_ index: Int) -> Bool {
        range.contains(index)
    }
}

/// What a diff cell's gutter can do with the reader's line selection.
///
/// One value rather than three closures on every cell: each arrangement builds
/// its cells the same way, and the pane is the only thing that can answer any
/// of them. Built once per pane render and handed to every cell, so a lazy
/// list of ten thousand rows allocates three closures, not thirty thousand.
struct DiffGutterActions {
    /// A gutter was clicked: the row's flat index, and whether shift was held
    /// (extend the run) rather than starting a new one.
    let select: (Int, Bool) -> Void
    /// Copy the current run, or this row alone when it is outside the run.
    let copy: (Int) -> Void
    /// Take the whole diff, for the reader who wants all of it.
    let selectAll: () -> Void
}

/// Which line-number columns a diff cell draws.
///
/// The only structural difference between the two arrangements at row level:
/// the unified row carries both of a line's numbers, while a split cell shows
/// the one belonging to its own side.
enum DiffGutter {
    /// Old and new, in that order — the unified layout's two columns.
    case both(old: Int32?, new: Int32?)
    /// A single column: the old number on the left of a split row, the new
    /// number on its right.
    case one(Int32?)
}

/// The one cell both diff arrangements are built from: line numbers, the
/// change glyph, and the line's content with its syntax colour and intra-line
/// backplate.
///
/// Unified gives it both numbers and the full pane width; split gives it one
/// number and places two of them side by side. Everything else — the glyphs,
/// the row tints, the tab expansion, the token styling — is written once here,
/// which is what keeps the two arrangements from ever describing the same line
/// differently.
struct DiffLineCell: View {
    /// The line this cell shows, or `nil` for a split-layout filler: the other
    /// column has a line here and this side has none.
    let line: DiffLine?
    /// The line's tokens, empty while phase two is still in flight.
    let tokens: [Token]
    let gutter: DiffGutter
    /// This cell's flat row index — what its gutter picks out, and what a copy
    /// range is built from. `nil` for a filler cell, which stands for no row at
    /// all and so selects nothing.
    let rowIndex: Int?
    /// The run this cell's gutter joins or extends. Passed whole rather than as
    /// a `Bool` so "is this row in the run?" is decided in one place.
    let selection: DiffLineSelection?
    let palette: DiffPalette
    /// Tab stop width in columns, from the shared `tab_size` setting.
    let tabSize: Int
    let actions: DiffGutterActions

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            switch gutter {
            case .both(let old, let new):
                lineNumber(old)
                lineNumber(new)
            case .one(let number):
                lineNumber(number)
            }

            Text(glyph)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(glyphColor)
                // Chrome, not content. A SwiftUI text selection cannot leave
                // the `Text` it began in, so the only thing this excludes is a
                // drag *started* on the glyph, which would otherwise put a
                // stray `+` on the clipboard. Cheap, and it costs nothing:
                // there is no cross-row selection here for it to interrupt.
                .textSelection(.disabled)
                .frame(width: 16)

            Text(attributedContent)
                .font(.system(size: 12, design: .monospaced))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.trailing, 12)
        }
        .padding(.vertical, 1)
        // Claims the row's full height as well as its width, so a cell whose
        // counterpart wrapped onto three lines carries its tint down all
        // three rather than leaving a stripe of pane showing under it.
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        // Two layers, not one: the selection wash lies *over* the row tint, so
        // a selected added line still reads as added.
        .background {
            background
            if isSelected {
                palette.lineSelection
            }
        }
    }

    private var isSelected: Bool {
        guard let rowIndex, let selection else { return false }
        return selection.contains(rowIndex)
    }

    /// One line-number column, and the surface the line selection is made on.
    ///
    /// The gutter is the line handle and the content is the text handle, the
    /// way GitHub's diff splits them — and here it is a necessity rather than a
    /// convention. A SwiftUI text selection lives inside one `Text` and cannot
    /// cross into the next, and this pane draws one `Text` per line, so a drag
    /// can never select more than the line it started in (**D-22**). Picking
    /// out several lines therefore has to be *asked for*, and the gutter is
    /// where it is asked.
    @ViewBuilder
    private func lineNumber(_ number: Int32?) -> some View {
        let label = Text(number.map(String.init) ?? "")
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.quaternary)
            // See the glyph: the numbers are the viewer's, not the file's.
            .textSelection(.disabled)
            .frame(width: 40, alignment: .trailing)
            .padding(.trailing, 4)

        if let rowIndex {
            label.overlay { handle(for: rowIndex) }
        } else {
            label
        }
    }

    /// The gutter's hit pad: an invisible layer over the number, carrying every
    /// gesture the line selection needs.
    ///
    /// A layer rather than modifiers on the number itself, so the gestures and
    /// the text they sit over stay independent: the pad is what a click hits,
    /// and the `Text` under it is only ever drawn. It is also the seam a real
    /// fix for D-22 would widen — a drag that selects lines belongs on this
    /// pad, once there is a row map to resolve it against.
    private func handle(for rowIndex: Int) -> some View {
        // The whole column, not just the digits: 40 pt is what a pointer can
        // actually hit, and the digits are 2–5 of them.
        Color.clear
            .contentShape(.rect)
            // Shift first and at high priority — `.modifiers` recognizes only
            // while the key is held, so a plain click falls straight through to
            // the tap below it.
            .highPriorityGesture(
                TapGesture().modifiers(.shift).onEnded { actions.select(rowIndex, true) }
            )
            .onTapGesture { actions.select(rowIndex, false) }
            .help("Click to select this line, ⇧-click to extend the selection")
            // The menu is on the gutter alone. Right-clicking the content has
            // to keep reaching the text selection's own menu, which is the
            // other half of the same split.
            .contextMenu {
                Button(copyTitle(for: rowIndex)) { actions.copy(rowIndex) }
                Button("Select All Lines", action: actions.selectAll)
            }
    }

    /// Names what the menu would actually copy, which is the run when this row
    /// is inside it and this row alone when it is not — the rule every
    /// list-with-a-context-menu follows.
    private func copyTitle(for rowIndex: Int) -> String {
        guard let selection, selection.contains(rowIndex), selection.range.count > 1 else {
            return "Copy Line"
        }
        return "Copy \(selection.range.count) Lines"
    }

    private var glyph: String {
        switch line?.lineType {
        case .add: "+"
        case .delete: "−"
        default: ""
        }
    }

    private var glyphColor: Color {
        switch line?.lineType {
        case .add: palette.addGlyph
        case .delete: palette.removeGlyph
        default: .clear
        }
    }

    private var background: Color {
        guard let line else { return palette.fillerBackground }
        return switch line.lineType {
        case .add: palette.addRowBackground
        case .delete: palette.removeRowBackground
        default: .clear
        }
    }

    private var attributedContent: AttributedString {
        guard let line else { return AttributedString() }
        return DiffLineText.attributed(
            content: line.content,
            tokens: tokens,
            intra: line.intraLineDiff,
            lineType: line.lineType,
            palette: palette,
            tabSize: tabSize
        )
    }
}

/// The `@@ -a,b +c,d @@ context` separator, drawn full width in both
/// arrangements — it describes the whole hunk, so in the split layout it spans
/// the two columns rather than pairing with anything.
struct DiffHunkBand: View {
    let text: String
    /// Whether the run the reader picked out in the gutter reaches this band.
    /// It has no line number of its own to click, but a run that spans it
    /// carries it — core copies a `@@` header as its own text — so it has to
    /// look carried rather than leaving a gap through the highlight.
    let isSelected: Bool
    let palette: DiffPalette

    var body: some View {
        Text(text)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                Rectangle().fill(.quaternary.opacity(0.5))
                if isSelected {
                    palette.lineSelection
                }
            }
    }
}

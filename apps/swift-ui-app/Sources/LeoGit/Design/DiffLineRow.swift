import SwiftUI

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
    let palette: DiffPalette
    /// Tab stop width in columns, from the shared `tab_size` setting.
    let tabSize: Int

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
        .background(background)
    }

    private func lineNumber(_ number: Int32?) -> some View {
        Text(number.map(String.init) ?? "")
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.quaternary)
            .frame(width: 40, alignment: .trailing)
            .padding(.trailing, 4)
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

    var body: some View {
        Text(text)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.5))
    }
}

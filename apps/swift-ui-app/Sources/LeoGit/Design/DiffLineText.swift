import SwiftUI

/// Builds the styled text for one diff line from core's tokens.
///
/// The native counterpart of core's `render.rs`: where that module collapses
/// tokens into `<span class="syn-*">` HTML for the WebView, this maps the same
/// tokens onto `AttributedString` runs. Token `start`/`end` and
/// `IntraLineRange` are code-point indices into the line's `content` — the
/// `unicodeScalars` view, never `characters`, whose grapheme clusters can
/// span several code points. Lines containing tabs are expanded to spaces
/// first (SwiftUI `Text` honours no tab stops), with every range remapped to
/// the expanded string.
enum DiffLineText {
    /// One diff line's content with syntax colour and the intra-line backplate
    /// applied. `tokens` may be empty (tokenization still in flight, or
    /// nothing to say) — the backplate still applies, so the two-phase render
    /// only ever *adds* colour.
    ///
    /// `tabSize` is the shared `tab_size` setting: tabs are expanded to the
    /// next tab stop before styling (see `expandingTabs`), and every incoming
    /// range — indices into the *original* content — is remapped to match.
    static func attributed(
        content: String,
        tokens: [Token],
        intra: IntraLineRange?,
        lineType: LineType,
        palette: DiffPalette,
        tabSize: Int
    ) -> AttributedString {
        let (rendered, map) = expandingTabs(in: content, tabSize: tabSize)
        var attributed = AttributedString(rendered)

        for token in tokens {
            guard
                let range = scalarRange(
                    in: attributed,
                    start: mapped(Int(token.start), through: map),
                    end: mapped(Int(token.end), through: map)
                )
            else {
                continue
            }
            let tokenClass = token.class
            if let color = palette.color(for: tokenClass) {
                attributed[range].foregroundColor = color
            }
            switch tokenClass {
            case .comment, .quote, .emphasis:
                attributed[range].inlinePresentationIntent = .emphasized
            case .strong, .heading:
                attributed[range].inlinePresentationIntent = .stronglyEmphasized
            case .strikethrough:
                attributed[range].strikethroughStyle = .single
            case .link:
                attributed[range].underlineStyle = .single
            default:
                break
            }
        }

        // Added and deleted rows carry a backplate behind just the characters
        // that differ from their counterpart line. Layered after the tokens so
        // it composes with (not replaces) syntax colour, matching render.rs.
        if let intra, intra.length > 0, lineType == .add || lineType == .delete,
            let range = scalarRange(
                in: attributed,
                start: mapped(Int(intra.start), through: map),
                end: mapped(Int(intra.start + intra.length), through: map)
            )
        {
            attributed[range].backgroundColor =
                lineType == .add ? palette.intraAddBackground : palette.intraRemoveBackground
        }

        return attributed
    }

    /// `content` with every tab replaced by the spaces reaching the next
    /// `tabSize`-column stop — CSS `tab-size` semantics, which the Tauri
    /// client gets from the browser for free. SwiftUI `Text` renders no
    /// paragraph-style attributes, so tab stops have to be baked into the
    /// string itself; the returned map (original code-point index → expanded
    /// index, one trailing entry for the end position) is what keeps token
    /// and intra-line ranges aligned afterwards. A `nil` map means the line
    /// had no tabs and every index is already right — the common case pays
    /// one `contains` scan and allocates nothing. Known divergence, noted in
    /// the plan: text copied from an expanded line carries spaces where the
    /// WebView preserves the tab characters.
    private static func expandingTabs(in content: String, tabSize: Int) -> (String, [Int]?) {
        guard content.contains("\t") else { return (content, nil) }
        // `tab_size = 0` in the file would trap the stride math; CSS clamps
        // the same way (a zero renders tabs as zero-width, which as columns
        // means "the next stop is the next column").
        let stop = max(tabSize, 1)
        var expanded = String.UnicodeScalarView()
        var map: [Int] = []
        map.reserveCapacity(content.unicodeScalars.count + 1)
        var column = 0
        for scalar in content.unicodeScalars {
            map.append(column)
            if scalar == "\t" {
                let width = stop - column % stop
                expanded.append(contentsOf: repeatElement(" ", count: width))
                column += width
            } else {
                expanded.append(scalar)
                column += 1
            }
        }
        map.append(column)
        return (String(expanded), map)
    }

    /// Moves a code-point index in the original content into the expanded
    /// string. Past-the-end indices clamp to the end; negatives pass through
    /// unmapped so `scalarRange` still rejects a malformed token.
    private static func mapped(_ index: Int, through map: [Int]?) -> Int {
        guard let map, index >= 0 else { return index }
        return map[min(index, map.count - 1)]
    }

    /// Converts a code-point `[start, end)` range into attributed-string
    /// indices, clamped to the content's length. Returns `nil` for an empty or
    /// out-of-bounds range so a malformed token degrades to plain text rather
    /// than trapping.
    private static func scalarRange(
        in attributed: AttributedString,
        start: Int,
        end: Int
    ) -> Range<AttributedString.Index>? {
        let scalars = attributed.unicodeScalars
        let count = scalars.count
        guard start >= 0, start < end, start < count else { return nil }
        let clampedEnd = min(end, count)
        let lower = scalars.index(scalars.startIndex, offsetBy: start)
        let upper = scalars.index(lower, offsetBy: clampedEnd - start)
        return lower..<upper
    }
}

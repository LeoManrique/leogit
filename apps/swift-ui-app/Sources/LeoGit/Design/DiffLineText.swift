import SwiftUI

/// Builds the styled text for one diff line from core's tokens.
///
/// The native counterpart of core's `render.rs`: where that module collapses
/// tokens into `<span class="syn-*">` HTML for the WebView, this maps the same
/// tokens onto `AttributedString` runs. Token `start`/`end` and
/// `IntraLineRange` are code-point indices into the line's `content`, which is
/// exactly `AttributedString.unicodeScalars` — never the `characters` view,
/// whose grapheme clusters can span several code points.
enum DiffLineText {
    /// One diff line's content with syntax colour and the intra-line backplate
    /// applied. `tokens` may be empty (tokenization still in flight, or
    /// nothing to say) — the backplate still applies, so the two-phase render
    /// only ever *adds* colour.
    static func attributed(
        content: String,
        tokens: [Token],
        intra: IntraLineRange?,
        lineType: LineType,
        palette: DiffPalette
    ) -> AttributedString {
        var attributed = AttributedString(content)

        for token in tokens {
            guard let range = scalarRange(in: attributed, start: Int(token.start), end: Int(token.end)) else {
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
                start: Int(intra.start),
                end: Int(intra.start + intra.length)
            )
        {
            attributed[range].backgroundColor =
                lineType == .add ? palette.intraAddBackground : palette.intraRemoveBackground
        }

        return attributed
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

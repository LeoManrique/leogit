import SwiftUI

/// Presentation for core's `FileStatus`.
///
/// The glyph and the name come from core, so the two clients cannot drift on
/// them again — they had, on the conflicted row, which is exactly the one the
/// user most needs to recognize. Only the colour lives here: it is the one
/// genuinely per-platform choice, resolved against the system palette.
extension FileStatus {
    /// Core's table, fetched once. Five entries, so the lookup below is a
    /// scan; crossing the bridge per row per repaint would not be.
    private static let styles: [FileStatusStyle] = fileStatusStyles()

    private var style: FileStatusStyle? {
        Self.styles.first { $0.status == self }
    }

    /// Single-letter badge, in git's own porcelain vocabulary (`U` for a
    /// conflict — "unmerged" is git's word for it).
    var letter: String { style?.letter ?? "?" }

    /// The status's name — the badge's accessible label, and the word any
    /// prose about the row should use.
    var label: String { style?.label ?? "Changed" }

    var tint: Color {
        switch self {
        case .new: .green
        case .modified: .orange
        case .deleted: .red
        case .renamed: .blue
        // Deliberately not red: red already means Deleted, and a glance down
        // the list has to separate "you deleted this" from "git couldn't merge
        // this" — opposite actions, one of which blocks the commit.
        case .conflicted: .purple
        }
    }
}

/// The coloured letter badge shown beside each changed file.
struct FileStatusBadge: View {
    let status: FileStatus

    var body: some View {
        Text(status.letter)
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .foregroundStyle(status.tint)
            .frame(width: 18, height: 18)
            .background(status.tint.opacity(0.15), in: .rect(cornerRadius: 4))
            .accessibilityLabel(status.label)
    }
}

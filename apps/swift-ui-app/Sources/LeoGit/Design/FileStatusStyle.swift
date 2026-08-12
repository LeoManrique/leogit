import SwiftUI

/// Presentation for core's `FileStatus`, kept in one place so the badge and any
/// future views cannot disagree about what a status looks like.
extension FileStatus {
    /// Single-letter badge, matching git's own porcelain vocabulary.
    var letter: String {
        switch self {
        case .new: "A"
        case .modified: "M"
        case .deleted: "D"
        case .renamed: "R"
        case .conflicted: "!"
        }
    }

    var tint: Color {
        switch self {
        case .new: .green
        case .modified: .orange
        case .deleted: .red
        case .renamed: .blue
        case .conflicted: .purple
        }
    }

    var label: String {
        switch self {
        case .new: "Added"
        case .modified: "Modified"
        case .deleted: "Deleted"
        case .renamed: "Renamed"
        case .conflicted: "Conflicted"
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

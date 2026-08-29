import SwiftUI

extension Color {
    /// The colour of an unfinished merge, wherever one shows: the `U` badge on
    /// a conflicted file, and the branch chip's `· merging` suffix.
    ///
    /// Deliberately not red — red already means Deleted, and a glance has to
    /// separate "you deleted this" from "git couldn't merge this", opposite
    /// actions of which only one blocks the commit. STYLE.md's
    /// `--status-purple`, and named here once so the two places a merge
    /// surfaces cannot drift apart the way the letter itself once did.
    static let merging = Color.purple
}

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
        case .conflicted: .merging
        }
    }
}

/// The badge beside a changed file: git's status letter on a soft tint plate,
/// or a ↪ link glyph on an entry that is a *repository* rather than a file.
///
/// The glyph **replaces** the letter rather than sitting beside it, because the
/// letter is the part that would be wrong: an embedded repository reports as
/// untracked and a dirty submodule as modified, and both readings invite
/// exactly the expectation that gets people — that the folder's files are what
/// goes into the commit. Only a pointer ever does, and for a dirty submodule
/// not even that.
///
/// Its colour splits on which of the two it is: the accent for an embedded
/// repository, which *is* committable as a gitlink, and muted for a dirty
/// submodule, which the parent repository cannot stage at all — the same
/// "inactive but still selectable to view" treatment its checkbox and filename
/// get (STYLE.md).
struct FileStatusBadge: View {
    let file: FileEntry

    var body: some View {
        Text(glyph)
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .foregroundStyle(tint)
            .frame(width: 18, height: 18)
            .background(tint.opacity(0.15), in: .rect(cornerRadius: 4))
            .accessibilityLabel(label)
            .help(help)
    }

    /// Nested repositories are the exception to git's vocabulary, so they are
    /// the exception here; everything else is core's letter.
    private var glyph: String { isRepositoryEntry ? "↪" : file.status.letter }

    private var tint: Color {
        if file.submoduleDirty { return .secondary }
        // A fixed blue, matching the Tauri badge's `--status-blue`, rather than
        // the accent: the accent follows a system preference, so the same row
        // would read differently on two machines and could land on any of the
        // status hues. It shares blue with Renamed on purpose — the glyph is
        // what separates them, and there are only so many usable hues.
        return file.embedded ? .blue : file.status.tint
    }

    private var label: String {
        if file.submoduleDirty { return "Dirty submodule" }
        return file.embedded ? "Embedded repository" : file.status.label
    }

    /// The badge carries the explanation, since it is the part that stopped
    /// reading like an ordinary change. A plain row falls back to the status's
    /// own name; the path's tooltip is the path's business.
    private var help: String { file.repositoryEntryHint ?? file.status.label }

    private var isRepositoryEntry: Bool { file.embedded || file.submoduleDirty }
}

extension FileEntry {
    /// Why this row is not an ordinary change, in one sentence — `nil` for a
    /// plain file.
    ///
    /// Lives here, in one place, because two surfaces have to say it and they
    /// must not drift: the row's ↪ badge, and — for a dirty submodule — the
    /// disabled checkbox, which is the control someone actually clicks when
    /// they want to know why they can't commit it. Both clients use these
    /// exact sentences.
    var repositoryEntryHint: String? {
        if submoduleDirty {
            return "This submodule has uncommitted changes that must be committed "
                + "inside the submodule before they can be part of this repository."
        }
        if embedded {
            return "Nested Git repository — commits as a link, not its files"
        }
        return nil
    }
}

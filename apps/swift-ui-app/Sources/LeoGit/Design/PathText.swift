import AppKit
import SwiftUI

/// Shortens a repo-relative path to a character budget, filename first.
///
/// The rule (ported from the Tauri client's `PathText.svelte`): the directory
/// gives way first, collapsing to a trailing `…/` bridge — but never below a
/// first-letter `b…/` hint, so a nested file can't be mistaken for a root file.
/// Only once that hint plus the whole filename still won't fit does the
/// filename itself middle-truncate.
///
/// The two parts are built here rather than by splitting an already-shortened
/// string, so directory characters can never end up painted as filename or the
/// reverse. Budgets count `Character`s — what the reader sees — which is why
/// `TruncatedPath` carries the pieces instead of an index.
enum PathTruncation {
    /// A path split into its muted directory prefix (empty, or ending in `/`)
    /// and its filename, each already shortened.
    struct TruncatedPath: Equatable {
        let directory: String
        let name: String

        var text: String { directory + name }
    }

    /// Splits `path` and shortens it to at most `budget` characters. A budget
    /// at or above the full length returns the path untouched.
    static func truncate(_ path: String, budget: Int) -> TruncatedPath {
        let separator = path.lastIndex(of: "/")
        let directory = separator.map { String(path[...$0]) } ?? ""
        let name = separator.map { String(path[path.index(after: $0)...]) } ?? path

        if path.count <= budget {
            return TruncatedPath(directory: directory, name: name)
        }
        guard budget > 0 else {
            return TruncatedPath(directory: "", name: "")
        }

        if !directory.isEmpty {
            if name.count + 3 <= budget {
                // Keep as much of the directory's head as fits, then bridge to
                // the filename. Two characters pay for the "…/" bridge itself.
                let keep = budget - name.count - 2
                return TruncatedPath(directory: String(directory.prefix(keep)) + "…/", name: name)
            }
            // A directory short enough to fit inside the hint's own footprint
            // shows whole — abbreviating it would save nothing.
            let hint = directory.count <= 3 ? directory : String(directory.prefix(1)) + "…/"
            if budget > hint.count {
                return TruncatedPath(
                    directory: hint,
                    name: middleTruncated(name, budget: budget - hint.count)
                )
            }
            // Embedded repositories arrive as a directory entry with a trailing
            // slash, so there is no filename to protect.
            if name.isEmpty {
                return TruncatedPath(directory: "…", name: "")
            }
        }
        return TruncatedPath(directory: "", name: middleTruncated(name, budget: budget))
    }

    /// Drops characters from the middle, keeping both ends — a filename's
    /// extension identifies it as much as its stem does.
    private static func middleTruncated(_ value: String, budget: Int) -> String {
        if value.count <= budget { return value }
        guard budget > 0 else { return "" }
        guard budget > 1 else { return "…" }

        let half = Double(budget - 1) / 2
        let head = value.prefix(Int(half.rounded(.down)))
        let tail = value.suffix(Int(half.rounded(.up)))
        return "\(head)…\(tail)"
    }
}

/// One line of path: muted directory, then the filename in body colour, shrunk
/// to whatever width the row can spare.
///
/// SwiftUI's own `.truncationMode` can't express the rule in `PathTruncation` —
/// it has no idea which half of the string carries the file's identity — so the
/// fit is measured here: a binary search for the largest budget that still
/// draws inside the available width, using the very font the label renders
/// with. That mirrors the hidden measuring span in `PathText.svelte`.
///
/// The view is greedy horizontally (the Svelte component's `flex: 1 1 0`), so
/// its width never depends on its own text and the measurement can't feed back
/// into layout. Tooltips are left to the caller, which usually has a whole row
/// to attach one to.
struct PathText: View {
    let path: String

    /// Used to draw *and* to measure, so what is measured is what is rendered.
    var font: NSFont = .preferredFont(forTextStyle: .body)

    /// Width of the space the row gave us; zero until the first layout pass.
    @State private var availableWidth: CGFloat = 0

    /// Breathing room so the last glyph never sits flush against the edge.
    private static let trailingPad: CGFloat = 2

    var body: some View {
        Text(styled(fittedParts))
            .font(Font(font as CTFont))
            .lineLimit(1)
            // Only reachable on the first frame, before the width is known —
            // an over-long path is clipped by SwiftUI rather than flashing an
            // empty row, and the measured fit replaces it immediately.
            .truncationMode(.middle)
            .frame(maxWidth: .infinity, alignment: .leading)
            .onGeometryChange(for: CGFloat.self) { proxy in
                proxy.size.width
            } action: { width in
                availableWidth = width
            }
    }

    /// Directory muted, filename in body colour — the split exists so the eye
    /// lands on the file's own name, not on the folders leading to it.
    private func styled(_ parts: PathTruncation.TruncatedPath) -> AttributedString {
        var directory = AttributedString(parts.directory)
        directory.foregroundColor = .secondary
        var name = AttributedString(parts.name)
        name.foregroundColor = .primary
        return directory + name
    }

    private var fittedParts: PathTruncation.TruncatedPath {
        let available = availableWidth - Self.trailingPad
        // Unknown width, or the whole path fits: nothing to shorten.
        guard available > 0, width(of: path) > available else {
            return PathTruncation.truncate(path, budget: path.count)
        }

        var low = 1
        var high = path.count
        var best = 1
        while low <= high {
            let budget = (low + high) / 2
            if width(of: PathTruncation.truncate(path, budget: budget).text) <= available {
                best = budget
                low = budget + 1
            } else {
                high = budget - 1
            }
        }
        return PathTruncation.truncate(path, budget: best)
    }

    private func width(of text: String) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }
}

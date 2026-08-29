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
/// draws inside the available width, using the very fonts the label renders
/// with. That mirrors the hidden measuring span in `PathText.svelte`.
///
/// **Every face that is drawn is also measured**, which is why `nameWeight`
/// exists as a parameter rather than as a `.fontWeight` the caller stacks on
/// top: a medium filename is wider than a regular one, and a fit measured in
/// the lighter face would overflow the row it just promised to fit.
///
/// The search runs only when one of its inputs moves — the path, the measured
/// width, or either face — and its result is held in state. A body evaluation is
/// not one of those inputs: a hover, a selection change, or a checkbox toggle
/// repaints every visible row, and re-deriving the fit from each of those
/// charged ~log₂(path) text measurements per row per repaint for an answer that
/// could not have changed.
///
/// The view is greedy horizontally (the Svelte component's `flex: 1 1 0`), so
/// its width never depends on its own text and the measurement can't feed back
/// into layout.
struct PathText: View {
    let path: String

    /// Used to draw *and* to measure, so what is measured is what is rendered.
    var font: NSFont = .preferredFont(forTextStyle: .body)

    /// A heavier face for the filename — the cue on a row that is going into
    /// the next commit. `nil` leaves it at `font`.
    var nameWeight: NSFont.Weight?

    /// Mute the filename as well as the directory, for a path that is not the
    /// one the eye should land on: a rename's pre-rename side, which the arrow
    /// points *away* from, and a dirty submodule, which the parent repository
    /// cannot stage.
    var isMuted = false

    /// Width of the space the row gave us; zero until the first layout pass.
    @State private var availableWidth: CGFloat = 0

    /// The last fit, held rather than re-derived. `nil` before the first
    /// measurement, which renders the whole path — see `body`.
    @State private var fitted: PathTruncation.TruncatedPath?

    /// Breathing room so the last glyph never sits flush against the edge.
    private static let trailingPad: CGFloat = 2

    var body: some View {
        Text(styled(fitted ?? PathTruncation.truncate(path, budget: path.count)))
            .lineLimit(1)
            // Only reachable on the first frame, before the width is known —
            // an over-long path is clipped by SwiftUI rather than flashing an
            // empty row, and the measured fit replaces it immediately.
            .truncationMode(.middle)
            // The tooltip is the path the row could not show in full. A row
            // that fits gets none: a tooltip repeating what is already on
            // screen trains people to ignore the ones that say something.
            //
            // **Innermost, and deliberately so.** It is a conditional modifier,
            // so the subtree it wraps is rebuilt when truncation starts or
            // stops — which on a long path is once per appearance, since the
            // first frame has no measurement yet. Everything that owns state or
            // fires on appear is applied *outside* it, where that rebuild
            // cannot reach and re-run the search with nothing changed.
            .modifier(TruncationTooltip(path: isTruncated ? path : nil))
            .frame(maxWidth: .infinity, alignment: .leading)
            .onGeometryChange(for: CGFloat.self) { proxy in
                proxy.size.width
            } action: { width in
                availableWidth = width
            }
            // One trigger for every input, so a change to any of them refits
            // and a change to none of them cannot. `initial` is what seeds the
            // very first fit — the width is still zero then, which `fit()`
            // answers with the whole path, and the geometry that follows
            // immediately supplies the real one.
            .onChange(of: inputs, initial: true) { fitted = fit() }
    }

    /// Everything the fit depends on, so `onChange` can watch it as one value.
    private struct Inputs: Equatable {
        let path: String
        let width: CGFloat
        let font: NSFont
        let nameFont: NSFont
    }

    private var inputs: Inputs {
        Inputs(path: path, width: availableWidth, font: font, nameFont: nameFont)
    }

    /// The filename's face. Derived from `font`'s size so the two stay in step.
    private var nameFont: NSFont {
        guard let nameWeight else { return font }
        return .systemFont(ofSize: font.pointSize, weight: nameWeight)
    }

    /// Whether the fit had to drop characters. False until measured, so a row
    /// mid-first-frame never claims a tooltip it may not need.
    private var isTruncated: Bool {
        fitted.map { $0.text.count < path.count } ?? false
    }

    /// Directory muted, filename in body colour — the split exists so the eye
    /// lands on the file's own name, not on the folders leading to it. A muted
    /// row gives up that emphasis on purpose.
    private func styled(_ parts: PathTruncation.TruncatedPath) -> AttributedString {
        var directory = AttributedString(parts.directory)
        directory.font = Font(font as CTFont)
        directory.foregroundColor = .secondary

        var name = AttributedString(parts.name)
        name.font = Font(nameFont as CTFont)
        name.foregroundColor = isMuted ? .secondary : .primary

        return directory + name
    }

    /// The largest budget whose rendered width still fits, by binary search.
    private func fit() -> PathTruncation.TruncatedPath {
        let whole = PathTruncation.truncate(path, budget: path.count)
        let available = availableWidth - Self.trailingPad
        // Unknown width, or the whole path fits: nothing to shorten.
        guard available > 0, width(of: whole) > available else { return whole }

        var low = 1
        var high = path.count
        var best = 1
        while low <= high {
            let budget = (low + high) / 2
            if width(of: PathTruncation.truncate(path, budget: budget)) <= available {
                best = budget
                low = budget + 1
            } else {
                high = budget - 1
            }
        }
        return PathTruncation.truncate(path, budget: best)
    }

    /// Each half in its own face — the same two the label draws with.
    private func width(of parts: PathTruncation.TruncatedPath) -> CGFloat {
        (parts.directory as NSString).size(withAttributes: [.font: font]).width
            + (parts.name as NSString).size(withAttributes: [.font: nameFont]).width
    }
}

/// `.help` that can be absent: an empty tooltip string still reserves a
/// tooltip, and the point of this one is that most rows have none.
private struct TruncationTooltip: ViewModifier {
    let path: String?

    func body(content: Content) -> some View {
        if let path {
            content.help(path)
        } else {
            content
        }
    }
}

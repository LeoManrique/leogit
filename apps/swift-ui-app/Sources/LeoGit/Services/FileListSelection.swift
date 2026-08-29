import Foundation

/// Which file a detail pane shows, given the rows a changed-file list has
/// highlighted. One rule, shared by the Changes tab and the commit detail.
///
/// The list's selection is a **set** — rows extend by shift-click and
/// shift-arrow — while a diff pane shows exactly one file, so the two are not
/// the same thing and the second cannot simply read the first. Selecting one
/// row is choosing a file, and the pane follows it. Extending a selection is
/// choosing a *group*, almost always to act on it, so the pane holds the file
/// it was already showing rather than jumping to whichever row the range
/// happened to end on — the diff someone is reading while they build a discard
/// selection around it is the one thing that must not move.
///
/// That last part is a deliberate divergence from the Tauri list, which
/// activates the shift-clicked row. Its gesture carries which row was clicked;
/// a `Set` does not, and inferring it would guess wrong on every selection that
/// grows by more than one row at a time.
enum FileListSelection {
    /// - Parameters:
    ///   - selection: the highlighted rows, by path.
    ///   - files: the list, in the order it is drawn.
    ///   - current: the file the pane is showing now.
    static func activePath(
        in selection: Set<String>,
        of files: [FileEntry],
        keeping current: String?
    ) -> String? {
        // A path that has left the list can't be shown whatever anything
        // claims — it was committed, discarded, or removed outside the app.
        // Every answer below is filtered through this, including the
        // single-row one: callers pre-prune today, but the next one will not
        // know it has to.
        func alive(_ path: String) -> String? {
            files.contains { $0.path == path } ? path : nil
        }

        // One row highlighted: that is the choice, always.
        if selection.count == 1, let only = selection.first { return alive(only) }

        let live = current.flatMap(alive)
        guard !selection.isEmpty else { return live }

        // Several rows: keep the open one when it is among them. Otherwise the
        // first in *list* order — never `selection.first`, which is a hash
        // order and would land on a different row from one launch to the next.
        if let live, selection.contains(live) { return live }
        return files.first { selection.contains($0.path) }?.path
    }
}

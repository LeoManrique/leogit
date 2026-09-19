import Foundation

/// The two selection rules every multi-row list in this client follows — the
/// changed-file lists and the commit list: which single row a detail pane
/// shows, and how a selection survives a reload. One place, so the lists cannot
/// answer the same situation differently. The gestures themselves — shift-click,
/// ⌘-click, shift-arrow, ⌘A — are AppKit's, through `List(selection:)`; the
/// Tauri client writes those by hand and keeps these same two rules beside them
/// (`utils/listSelection.ts`).
///
/// A list's selection is a **set** — rows extend by shift-click and
/// shift-arrow — while a detail pane shows exactly one row, so the two are not
/// the same thing and the second cannot simply read the first. Selecting one
/// row is choosing it, and the pane follows. Extending a selection is choosing
/// a *group*, almost always to act on it, so the pane holds the row it was
/// already showing rather than jumping to whichever row the range happened to
/// end on — the diff someone is reading while they build a discard selection
/// around it is the one thing that must not move.
///
/// That last part is a deliberate divergence from the Tauri lists, which show
/// the clicked row. Their gesture carries which row was clicked; a `Set` does
/// not, and inferring it would guess wrong on every selection that grows by
/// more than one row at a time.
enum ListSelection {
    /// The row a detail pane shows.
    ///
    /// - Parameters:
    ///   - selection: the highlighted rows, by id.
    ///   - rows: the list, in the order it is drawn.
    ///   - current: the row the pane is showing now.
    static func activeID<Row: Identifiable>(
        in selection: Set<Row.ID>,
        of rows: [Row],
        keeping current: Row.ID?
    ) -> Row.ID? {
        // A row that has left the list can't be shown whatever anything
        // claims — it was committed, discarded, rewritten away, or removed
        // outside the app. Every answer below is filtered through this,
        // including the single-row one: callers pre-prune today, but the next
        // one will not know it has to.
        func alive(_ id: Row.ID) -> Row.ID? {
            rows.contains { $0.id == id } ? id : nil
        }

        // One row highlighted: that is the choice, always.
        if selection.count == 1, let only = selection.first { return alive(only) }

        let live = current.flatMap(alive)
        guard !selection.isEmpty else { return live }

        // Several rows: keep the open one when it is among them. Otherwise the
        // first in *list* order — never `selection.first`, which is a hash
        // order and would land on a different row from one launch to the next.
        if let live, selection.contains(live) { return live }
        return rows.first { selection.contains($0.id) }?.id
    }

    /// The selection, kept describing rows that still exist.
    ///
    /// Two things, in order. Rows that have left the list are pruned, because a
    /// highlight pointing at nothing would still be counted by every action
    /// that reads it. Then, and only if nothing is left, the list re-seats on
    /// its first row. Those are the two conditions and no others (STYLE.md): a
    /// row the user chose is never overridden, and a reload that changes only a
    /// row's content is not a re-seat — callers trigger this on the *set of
    /// ids* changing.
    static func reseated<Row: Identifiable>(
        _ selection: Set<Row.ID>,
        in rows: [Row]
    ) -> Set<Row.ID> {
        let survivors = selection.intersection(rows.map(\.id))
        if survivors.isEmpty { return rows.first.map { [$0.id] } ?? [] }
        return survivors
    }

    /// The selected rows in the order the list draws them — what a context menu
    /// acts on. Never the set's own order: a menu that names a count, or a
    /// range of history, has to describe the rows as they appear, and
    /// `contextMenu(forSelectionType:)` can also hand back ids that have since
    /// left the list.
    static func targets<Row: Identifiable>(_ ids: Set<Row.ID>, in rows: [Row]) -> [Row] {
        rows.filter { ids.contains($0.id) }
    }
}

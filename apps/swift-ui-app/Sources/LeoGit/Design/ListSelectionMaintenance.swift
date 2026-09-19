import SwiftUI

extension View {
    /// Keep a multi-row list's selection, and the one row its detail pane
    /// shows, describing rows that exist — `ListSelection`'s rules, applied at
    /// the two moments they can stop being true. Attached by whoever owns the
    /// list's rows; the Changes and History sidebars both do, which is what
    /// keeps the two lists from answering the same situation differently.
    ///
    /// - Parameters:
    ///   - selection: the highlighted rows, by id.
    ///   - active: the row the detail pane shows, derived from `selection`.
    ///   - rows: the list, in the order it is drawn.
    func maintainsSelection<Row: Identifiable>(
        _ selection: Binding<Set<Row.ID>>,
        showing active: Binding<Row.ID?>,
        of rows: [Row]
    ) -> some View {
        modifier(ListSelectionMaintenance(rows: rows, selection: selection, active: active))
    }
}

private struct ListSelectionMaintenance<Row: Identifiable>: ViewModifier {
    let rows: [Row]
    @Binding var selection: Set<Row.ID>
    @Binding var active: Row.ID?

    func body(content: Content) -> some View {
        content
            // The *ids* changing, not the rows: a reload that changes only a
            // row's content is not a reason to touch what the user chose.
            .onChange(of: rows.map(\.id), initial: true) { settle(selection) }
            .onChange(of: selection) { old, new in
                // A list with rows keeps a selection. AppKit lets ⌘-click take
                // the last row out and a click below the rows clear them all;
                // both would leave the pane showing a row nothing marks, so the
                // selection that was there is put back. Written back from here
                // rather than refused in the binding's setter: the table has
                // already drawn the deselection by then, and only a state
                // change makes it draw the selection again.
                //
                // This cannot tell the user's click from an owner clearing the
                // set, so an owner that clears it while `rows` is non-empty is
                // undone too. The one that does — a repository switch — also
                // empties the rows in the same step, which is what makes its
                // clear stick.
                settle(new.isEmpty ? old : new)
            }
    }

    private func settle(_ wanted: Set<Row.ID>) {
        let next = ListSelection.reseated(wanted, in: rows)
        if next != selection { selection = next }
        let shown = ListSelection.activeID(in: next, of: rows, keeping: active)
        if shown != active { active = shown }
    }
}

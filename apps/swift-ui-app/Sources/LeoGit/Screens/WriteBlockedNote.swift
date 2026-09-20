import SwiftUI

/// The line a sheet shows while the write it confirms cannot start, because
/// another repository write holds the window's slot.
///
/// A sheet can sit open under someone else's write — the merge sheet under a
/// commit — and its button disables for as long. A disabled button that does
/// not say why reads as broken, and one left live is a click that does
/// nothing, so every such sheet shows this beside the disabled button. It is
/// live: it goes the moment the slot does.
///
/// Shown for *someone else's* write only. A sheet's own write is a different
/// state, which it words itself ("Merging…", "Discarding…").
struct WriteBlockedNote: View {
    var body: some View {
        Text(RepositoryWriteGate.busyMessage)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

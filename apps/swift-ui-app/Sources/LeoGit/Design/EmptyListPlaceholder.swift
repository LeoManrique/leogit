import SwiftUI

/// The faint centred line a sidebar list shows when it has no rows — the
/// Tauri lists' "No changes" / "No commits yet". Deliberately not a
/// `ContentUnavailableView`: the icon-and-headline treatment is sized for a
/// pane, and the pane-sized story ("The working tree is clean.") is the
/// detail side's to tell. Claims the whole slot so the composer or the tab
/// bar around it stay pinned where a populated list would leave them.
struct EmptyListPlaceholder: View {
    let text: String

    var body: some View {
        Text(text)
            .foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

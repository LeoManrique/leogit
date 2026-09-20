import SwiftUI

/// A refusal as it was written — git's, or core's when it refused before
/// asking git — inside the sheet that raised it: monospaced so git's ref names
/// line up, selectable so a name can be copied out of it, and scrollable so a
/// long hint block or a hook's output cannot push the buttons off the sheet
/// (STYLE.md).
struct RefusalText: View {
    let message: String

    var body: some View {
        ScrollView {
            Text(message)
                .font(.caption.monospaced())
                .foregroundStyle(.red)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxHeight: 120)
    }
}

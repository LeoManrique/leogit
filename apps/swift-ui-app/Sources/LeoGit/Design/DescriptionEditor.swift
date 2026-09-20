import SwiftUI

/// A commit message's description field — the native counterpart of the Tauri
/// textarea, shared by the composer and the squash sheet. It fills whatever
/// height its owner leaves it and scrolls with a scrollbar once the text
/// outgrows that.
///
/// `TextEditor` rather than a vertical-axis `TextField` because only the
/// editor is a real scroll view; it brings no bezel or placeholder of its own,
/// so both are drawn here to match the summary field above it. Disabling is the
/// owner's, through `.disabled(_:)`.
struct DescriptionEditor: View {
    @Binding var text: String

    var body: some View {
        ZStack(alignment: .topLeading) {
            TextEditor(text: $text)
                .font(.body)
                .scrollContentBackground(.hidden)
                .contentMargins(4, for: .scrollContent)
                .frame(maxHeight: .infinity)

            if text.isEmpty {
                Text("Description")
                    .foregroundStyle(Color(nsColor: .placeholderTextColor))
                    .padding(.top, 4)
                    // The editor's line fragment padding plus its content
                    // margin — keeps the prompt on the first character's spot.
                    .padding(.leading, 9)
                    .allowsHitTesting(false)
            }
        }
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.separator))
    }
}

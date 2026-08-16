import SwiftUI

/// The small stadium count pill used wherever a control carries a number —
/// the tab bar's changed-file count and the sync button's ahead/behind count.
/// One shared shape where the Tauri client hand-rolls two near-identical
/// pills: 16 pt tall, 18 pt minimum width so single digits sit centered,
/// tabular digits so a ticking count doesn't wobble.
struct CountBadge: View {
    let count: Int

    var body: some View {
        Text("\(count)")
            .font(.system(size: 11, weight: .medium))
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .padding(.horizontal, 5)
            .frame(minWidth: 18, minHeight: 16)
            .background(.quaternary, in: .capsule)
    }
}

import SwiftUI

/// The Changes tab's half of the main content: the selected file's
/// working-tree diff, or the reason there isn't one. Every branch claims the
/// whole slot — it shares a column with the terminal dock, and an empty state
/// left to its own size would let the dock float up to meet it.
struct ChangesDetailPane: View {
    let repoPath: String
    let files: [FileEntry]
    let selectedPath: String?
    /// Bumped by every status reload; the diff re-reads when it changes.
    let statusEpoch: Int

    var body: some View {
        Group {
            if let file = files.first(where: { $0.path == selectedPath }) {
                DiffView(repoPath: repoPath, file: file, target: .workingTree(epoch: statusEpoch))
            } else if files.isEmpty {
                ContentUnavailableView(
                    "No Changes",
                    systemImage: "checkmark.circle",
                    description: Text("The working tree is clean.")
                )
            } else {
                ContentUnavailableView(
                    "No File Selected",
                    systemImage: "doc.text",
                    description: Text("Select a file to see its changes.")
                )
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

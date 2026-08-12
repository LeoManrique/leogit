import SwiftUI

/// Working-tree changes for the open repository: the changed-file list on the
/// left, the selected file's diff on the right.
struct ChangesView: View {
    let repoPath: String
    let files: [FileEntry]
    let statusEpoch: Int

    /// Selection is the file's repo-relative path (`FileEntry.id`), so it
    /// survives a status reload that replaces every row value.
    @State private var selectedPath: String?

    var body: some View {
        if files.isEmpty {
            ContentUnavailableView(
                "No Changes",
                systemImage: "checkmark.circle",
                description: Text("The working tree is clean.")
            )
        } else {
            HSplitView {
                fileList
                    .frame(minWidth: 240, idealWidth: 300, maxWidth: 520)
                detail
                    .frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
            }
            .onChange(of: files.map(\.path), initial: true) {
                // Keep something selected: first file on arrival, and again
                // when a reload drops the previously selected path.
                if selectedPath == nil || !files.contains(where: { $0.path == selectedPath }) {
                    selectedPath = files.first?.path
                }
            }
        }
    }

    private var fileList: some View {
        List(files, selection: $selectedPath) { file in
            HStack(spacing: 10) {
                FileStatusBadge(status: file.status)

                VStack(alignment: .leading, spacing: 1) {
                    Text(file.displayName)
                        .font(.body)
                    if !file.displayDir.isEmpty {
                        Text(file.displayDir)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                Spacer(minLength: 0)

                // Entries the parent repo cannot stage — surfaced here so the
                // list never implies an action that would silently no-op.
                if file.submoduleDirty {
                    Text("submodule")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .help("Changes live inside the submodule and must be committed there")
                } else if file.embedded {
                    Text("embedded repo")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .help("A nested repository; committing stages a gitlink, not its files")
                }
            }
            .padding(.vertical, 2)
            .help(file.path)
        }
        .listStyle(.inset)
        .alternatingRowBackgrounds()
    }

    @ViewBuilder
    private var detail: some View {
        if let file = files.first(where: { $0.path == selectedPath }) {
            DiffView(repoPath: repoPath, file: file, statusEpoch: statusEpoch)
        } else {
            ContentUnavailableView(
                "No File Selected",
                systemImage: "doc.text",
                description: Text("Select a file to see its changes.")
            )
        }
    }
}

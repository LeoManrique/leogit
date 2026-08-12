import SwiftUI

/// Working-tree changes for the open repository.
struct ChangesView: View {
    let files: [FileEntry]

    var body: some View {
        if files.isEmpty {
            ContentUnavailableView(
                "No Changes",
                systemImage: "checkmark.circle",
                description: Text("The working tree is clean.")
            )
        } else {
            List(files) { file in
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
    }
}

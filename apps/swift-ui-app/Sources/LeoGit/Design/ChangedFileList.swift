import SwiftUI

/// The changed-file list shared by the Changes tab and the History detail:
/// one row per file — status badge, the one-line compacted path, and a tag on
/// entries the parent repository cannot stage. The Changes tab prepends its
/// include checkbox through `leading`; the History detail prepends nothing (a
/// past commit offers nothing to include or exclude). The Tauri client shares
/// the same component between both tabs (`FileList.svelte`, `showCheckbox`).
struct ChangedFileList<Leading: View>: View {
    let files: [FileEntry]
    @Binding var selectedPath: String?
    /// Built through the init's `@ViewBuilder` parameter.
    let leading: (FileEntry) -> Leading

    init(
        files: [FileEntry],
        selectedPath: Binding<String?>,
        @ViewBuilder leading: @escaping (FileEntry) -> Leading
    ) {
        self.files = files
        self._selectedPath = selectedPath
        self.leading = leading
    }

    var body: some View {
        List(files, selection: $selectedPath) { file in
            HStack(spacing: 10) {
                leading(file)

                FileStatusBadge(status: file.status)

                // Greedy, so it also supplies the gap before the trailing tag —
                // a Spacer here would split the slack with it and shorten the
                // path for no reason.
                PathText(path: file.path)

                // Entries the parent repo cannot stage — surfaced here so the
                // list never implies an action that would silently no-op.
                // `.fixedSize` keeps the tag whole: the path is the flexible
                // one, and it already knows how to shorten itself gracefully.
                if file.submoduleDirty {
                    tag(
                        "submodule",
                        help: "Changes live inside the submodule and must be committed there"
                    )
                } else if file.embedded {
                    tag(
                        "embedded repo",
                        help: "A nested repository; committing stages a gitlink, not its files"
                    )
                }
            }
            .padding(.vertical, 2)
            .help(file.path)
        }
        .listStyle(.inset)
        .alternatingRowBackgrounds()
    }

    private func tag(_ label: String, help: String) -> some View {
        Text(label)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .fixedSize()
            .help(help)
    }
}

extension ChangedFileList where Leading == EmptyView {
    /// A list with no leading accessory.
    init(files: [FileEntry], selectedPath: Binding<String?>) {
        self.init(files: files, selectedPath: selectedPath) { _ in EmptyView() }
    }
}

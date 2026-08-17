import SwiftUI

/// The changed-file list shared by the Changes tab and the History detail:
/// one row per file — status badge, the one-line compacted path, and a tag on
/// entries the parent repository cannot stage. The Changes tab prepends its
/// include checkbox through `leading` and hangs a right-click menu off `menu`;
/// the History detail passes neither (a past commit offers nothing to include,
/// exclude, discard, or ignore). The Tauri client shares the same component
/// between both tabs the same way (`FileList.svelte`, `showCheckbox` and its
/// optional `contextActions`).
struct ChangedFileList<Leading: View, Menu: View>: View {
    let files: [FileEntry]
    @Binding var selectedPath: String?
    /// Built through the init's `@ViewBuilder` parameter.
    let leading: (FileEntry) -> Leading
    /// Context-menu items for the right-clicked row.
    let menu: (FileEntry) -> Menu
    /// Whether `menu` builds anything. Attaching the modifier unconditionally
    /// would claim the right-click on lists that have no actions to offer.
    private let hasMenu: Bool

    private init(
        files: [FileEntry],
        selectedPath: Binding<String?>,
        hasMenu: Bool,
        leading: @escaping (FileEntry) -> Leading,
        menu: @escaping (FileEntry) -> Menu
    ) {
        self.files = files
        self._selectedPath = selectedPath
        self.hasMenu = hasMenu
        self.leading = leading
        self.menu = menu
    }

    init(
        files: [FileEntry],
        selectedPath: Binding<String?>,
        @ViewBuilder leading: @escaping (FileEntry) -> Leading,
        @ViewBuilder menu: @escaping (FileEntry) -> Menu
    ) {
        self.init(
            files: files,
            selectedPath: selectedPath,
            hasMenu: true,
            leading: leading,
            menu: menu
        )
    }

    var body: some View {
        if hasMenu {
            // The selection-aware modifier rather than one `.contextMenu` per
            // row: right-clicking a row makes it the selection first, so the
            // menu and the diff pane always describe the same file — the
            // re-select the Tauri list performs by hand in its own handler.
            list.contextMenu(forSelectionType: String.self) { paths in
                if let path = paths.first, let file = files.first(where: { $0.path == path }) {
                    menu(file)
                }
            }
        } else {
            list
        }
    }

    private var list: some View {
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

extension ChangedFileList where Menu == EmptyView {
    /// A list whose rows offer no context menu.
    init(
        files: [FileEntry],
        selectedPath: Binding<String?>,
        @ViewBuilder leading: @escaping (FileEntry) -> Leading
    ) {
        self.init(
            files: files,
            selectedPath: selectedPath,
            hasMenu: false,
            leading: leading,
            menu: { _ in EmptyView() }
        )
    }
}

extension ChangedFileList where Leading == EmptyView, Menu == EmptyView {
    /// A list with no leading accessory and no context menu.
    init(files: [FileEntry], selectedPath: Binding<String?>) {
        self.init(
            files: files,
            selectedPath: selectedPath,
            hasMenu: false,
            leading: { _ in EmptyView() },
            menu: { _ in EmptyView() }
        )
    }
}

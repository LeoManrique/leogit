import AppKit
import SwiftUI

/// The changed-file list shared by the Changes tab and the History detail:
/// one row per file — status badge (or a ↪ for a nested repository) and the
/// one-line compacted path, `old → new` when the file was renamed. The Changes
/// tab prepends its include checkbox through `leading` and hangs a right-click
/// menu off `menu`; the History detail passes neither (a past commit offers
/// nothing to include, exclude, discard, or ignore). The Tauri client shares
/// the same component between both tabs the same way (`FileList.svelte`,
/// `showCheckbox` and its optional `contextActions`).
///
/// The selection is a **set**: rows extend by shift-click and shift-arrow, and
/// a right-click inside a multi-row selection acts on all of it. Which file the
/// diff pane shows is the owner's business, not this view's — see
/// `ChangesSidebar`.
struct ChangedFileList<Leading: View, Menu: View>: View {
    let files: [FileEntry]

    /// Rows the user has highlighted, by `FileEntry.id`. Keyed by path so it
    /// survives a status reload that replaces every row value.
    @Binding var selection: Set<String>

    /// Whether a row is going into the next commit — a medium-weight filename,
    /// the one cue that separates *included* from merely *selected* without
    /// spending another column. `nil` where inclusion has no meaning.
    let isIncluded: ((FileEntry) -> Bool)?

    /// Toggle inclusion for the whole selection — Space, the highest-frequency
    /// action in the app. Returns whether it changed anything, so a press that
    /// could do nothing is passed on rather than swallowed. `nil` on a list with
    /// no checkboxes.
    let onToggleSelection: (() -> Bool)?

    /// Built through the init's `@ViewBuilder` parameter.
    let leading: (FileEntry) -> Leading

    /// Context-menu items for what the right-click is acting on: the whole
    /// selection when it started inside one, otherwise just the clicked row.
    /// SwiftUI decides which, and re-selects in the second case — so a menu
    /// raised over an unselected row can never act on a file the user is not
    /// looking at. Over a multi-row selection it acts on all of them, which the
    /// diff pane deliberately does not follow (`FileListSelection`).
    let menu: ([FileEntry]) -> Menu

    /// Whether `menu` builds anything. Attaching the modifier unconditionally
    /// would claim the right-click on lists that have no actions to offer.
    private let hasMenu: Bool

    private init(
        files: [FileEntry],
        selection: Binding<Set<String>>,
        isIncluded: ((FileEntry) -> Bool)?,
        onToggleSelection: (() -> Bool)?,
        hasMenu: Bool,
        leading: @escaping (FileEntry) -> Leading,
        menu: @escaping ([FileEntry]) -> Menu
    ) {
        self.files = files
        self._selection = selection
        self.isIncluded = isIncluded
        self.onToggleSelection = onToggleSelection
        self.hasMenu = hasMenu
        self.leading = leading
        self.menu = menu
    }

    init(
        files: [FileEntry],
        selection: Binding<Set<String>>,
        isIncluded: @escaping (FileEntry) -> Bool,
        onToggleSelection: @escaping () -> Bool,
        @ViewBuilder leading: @escaping (FileEntry) -> Leading,
        @ViewBuilder menu: @escaping ([FileEntry]) -> Menu
    ) {
        self.init(
            files: files,
            selection: selection,
            isIncluded: isIncluded,
            onToggleSelection: onToggleSelection,
            hasMenu: true,
            leading: leading,
            menu: menu
        )
    }

    var body: some View {
        if hasMenu {
            list.contextMenu(forSelectionType: String.self) { paths in
                // In list order, not set order: a menu that names a count has
                // to act on a stable set, and every caller reports paths.
                let targets = files.filter { paths.contains($0.path) }
                if !targets.isEmpty {
                    menu(targets)
                }
            }
        } else {
            list
        }
    }

    private var list: some View {
        List(files, selection: $selection) { file in
            HStack(spacing: 10) {
                leading(file)

                FileStatusBadge(file: file)

                pathLabel(for: file)
            }
            .padding(.vertical, 2)
        }
        .listStyle(.inset)
        .alternatingRowBackgrounds()
        // Space over the list toggles the whole selection. The row checkboxes
        // keep their own Space (AppKit gives a focused control the key first),
        // so the two never fight over one press.
        .onKeyPress(.space) {
            guard let onToggleSelection, onToggleSelection() else { return .ignored }
            return .handled
        }
    }

    /// A rename is two paths and the move between them, so it renders as both:
    /// the pre-rename side fully muted and the current name in the row's own
    /// treatment. Rendering only the destination — which is all git's status
    /// letter would leave — makes a rename indistinguishable from an add, and
    /// loses the one fact that matters about it.
    ///
    /// Both sides are greedy, so they split the row's slack evenly and each
    /// shortens under its own rule; a deep `from` path cannot crowd the `to`
    /// out of view.
    @ViewBuilder
    private func pathLabel(for file: FileEntry) -> some View {
        if let origPath = file.origPath {
            HStack(spacing: 4) {
                PathText(path: origPath, isMuted: true)
                Text("→")
                    .foregroundStyle(.secondary)
                    .fixedSize()
                currentPath(of: file)
            }
        } else {
            currentPath(of: file)
        }
    }

    private func currentPath(of file: FileEntry) -> PathText {
        PathText(
            path: file.path,
            nameWeight: isIncluded?(file) == true ? .medium : nil,
            // A dirty submodule reads as inactive throughout — muted name,
            // muted badge, disabled checkbox — because the parent repository
            // can do nothing with it, while the row stays selectable so its
            // state can still be looked at.
            isMuted: file.submoduleDirty
        )
    }
}

extension ChangedFileList where Leading == EmptyView, Menu == EmptyView {
    /// A list with no checkbox column and no context menu — a past commit's
    /// files, which offer nothing to include, exclude, discard, or ignore.
    init(files: [FileEntry], selection: Binding<Set<String>>) {
        self.init(
            files: files,
            selection: selection,
            isIncluded: nil,
            onToggleSelection: nil,
            hasMenu: false,
            leading: { _ in EmptyView() },
            menu: { _ in EmptyView() }
        )
    }
}

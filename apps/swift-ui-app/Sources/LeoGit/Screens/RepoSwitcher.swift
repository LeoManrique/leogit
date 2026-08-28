import SwiftUI

/// The toolbar repo chip and its popover: every known repository one click
/// away, with per-repo indicators — a dirty dot for uncommitted changes, ↓
/// for commits to pull, ↑ for commits to push — so the state of the whole
/// working set is visible before switching. The native counterpart of the
/// Tauri client's header chip + `RepoDropdown`.
///
/// The list itself is `RepoPickerList`, shared with the Welcome screen. What
/// belongs here is only what the popover adds: dismissing itself before it
/// hands an action on, and keeping the rows' badges fresh while it is open.
struct RepoSwitcher: View {
    let activePath: String
    let directory: RepoDirectoryStore
    let identifiers: RepoIdentifierStore

    /// Gates the open-popover badge sweep (`canRunRepoSweeps` — in practice
    /// only a network operation can block it while the popover is open).
    let policy: BackgroundSchedulingPolicy

    /// Non-nil while a transfer holds the network slot. It reaches the rows,
    /// not this button: browsing the list and reaching Clone contend with
    /// nothing, and only the *switch* would reset the sync UI from under a
    /// running operation.
    let switchBlockedReason: String?

    let onSelect: (String) -> Void
    let onClone: () -> Void
    let onChooseFolders: () -> Void

    @State private var isPresented = false

    var body: some View {
        Button {
            isPresented.toggle()
        } label: {
            Label(RepoDirectoryStore.displayName(of: activePath), systemImage: "folder")
        }
        // macOS toolbars render Labels icon-only by default; the active
        // repo's name is half the chip's value (and the toolbar title no
        // longer shows it).
        .labelStyle(.titleAndIcon)
        .help("Switch repository")
        .popover(isPresented: $isPresented, arrowEdge: .bottom) {
            RepoPickerList(
                activePath: activePath,
                directory: directory,
                identifiers: identifiers,
                switchBlockedReason: switchBlockedReason,
                listMaxHeight: 360,
                onSelect: { path in
                    isPresented = false
                    onSelect(path)
                },
                onClone: {
                    isPresented = false
                    onClone()
                },
                onChooseFolders: {
                    isPresented = false
                    onChooseFolders()
                }
            )
            .frame(width: 320)
            // Fresh list and fresh badges each time the popover opens, and
            // *concurrently*: the walk is a filesystem crawl over the scan
            // tree, while the sweep is two git reads per row, and running the
            // sweep behind the walk delayed every badge by the slower half.
            .task { await directory.refreshDirectory() }
            .task(id: directory.repos) {
                // Keyed on the rows so the sweep re-runs when the walk
                // publishes new ones, rather than filling only what the list
                // happened to hold when the popover opened.
                await directory.sweepVisible(activePath: activePath, policy: policy)
            }
        }
    }
}

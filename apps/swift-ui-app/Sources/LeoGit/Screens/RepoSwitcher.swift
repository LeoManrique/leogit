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
            // The repository's own name and nothing else. Deliberately not
            // owner-qualified: an `owner/` in front of it would be one more
            // word to read past on a chip whose whole job is to answer "which
            // repository" at a glance, and macOS draws toolbar labels in one
            // tone, so a prefix here cannot be shaded down out of the way.
            // Where two checkouts really do share a name, the picker's rows
            // disambiguate them — that is a list, and a list is where the
            // ambiguity actually bites.
            Label(identifiers.label(of: activePath), systemImage: "folder")
        }
        // macOS toolbars render Labels icon-only by default; the active
        // repo's name is half the chip's value (and the toolbar title no
        // longer shows it).
        .labelStyle(.titleAndIcon)
        // The one semibold in the bar. With the title removed this chip is the
        // only place the window says which repository is open, so it carries
        // the weight and the branch chip beside it stays regular — identity
        // first, then the detail.
        .font(.body.weight(.semibold))
        .help("Switch repository")
        // `label` prefers the remote's repository name and falls back to the
        // folder's, and the remote half is a `git config` read. Nothing else
        // asks for the *open* repository's — `RepoPickerList` primes the rows
        // it draws, which live behind this button — so without this the chip
        // would sit on the folder name until the picker was first opened, and
        // disagree with the row naming the same repository.
        .onChange(of: activePath, initial: true) { identifiers.ensure([activePath]) }
        .popover(isPresented: $isPresented, arrowEdge: .bottom) {
            RepoPickerList(
                activePath: activePath,
                directory: directory,
                identifiers: identifiers,
                switchBlockedReason: switchBlockedReason,
                height: .fill,
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
            // A declared size, not a fitted one — see `RepoPickerHeight`. The
            // width was already fixed for the same reason it always is: rows
            // are paths, and a list that widens with its longest one would
            // resize as the user types.
            .frame(width: 320, height: 440)
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

import SwiftUI

/// The app's menu bar.
///
/// Every chord the app answers is meant to have a home here, because a
/// keyboard shortcut that exists only as a button's key equivalent is
/// discoverable nowhere: the Tauri client can afford a `?` overlay listing
/// them, and the native answer to the same problem is the menu that macOS
/// already puts on every screen.
///
/// The menus are also *structurally* the better place for a chord. A menu key
/// equivalent is matched before the responder chain sees the event, so a
/// binding here cannot be swallowed by whatever holds focus — the embedded
/// terminal, a text field — which is the class of bug the Tauri client had to
/// fix by hand (D-16).
///
/// Items whose title or enablement depends on repository state arrive as
/// focused scene values rather than notifications: a notification can fire an
/// action but cannot label or disable it, and a menu item that lies about what
/// it does is worse than no shortcut at all. Their perform closures still hop
/// through the owning view, so the sheets and dialogs an action may open stay
/// with the control that a click would have used.

// MARK: - Repository

/// The menu-bar home of the toolbar's adaptive sync button: one item that
/// renames itself to whatever the button proposes — Publish, Publish Branch,
/// Pull, Push, Fetch — and runs the same closure under ⌘P.
struct RepositoryCommands: Commands {
    @FocusedValue(\.syncCommand) private var syncCommand: SyncCommand?

    var body: some Commands {
        CommandMenu("Repository") {
            // Titled for the neutral state while no repository is open, so
            // the item reads sensibly even though it's disabled there.
            Button(syncCommand?.title ?? "Fetch") {
                syncCommand?.perform()
            }
            .keyboardShortcut("p")
            .disabled(syncCommand?.isEnabled != true)
        }
    }
}

// MARK: - File

/// Clone is the only thing the File menu offers, and deliberately so: there is
/// no *Open Repository…*, because a repository list is what the scan paths
/// cover, and a one-off open would let the list disagree with the settings
/// that are supposed to define it (RM-2). A repository the paths miss arrives
/// by clone or by `leogit <dir>` and keeps its row from then on.
struct FileCommands: Commands {
    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Clone Repository…") {
                NotificationCenter.default.post(name: .leogitCloneRequested, object: nil)
            }
            .keyboardShortcut("o", modifiers: [.shift, .command])
        }
    }
}

// MARK: - View

/// ⌘1 / ⌘2 for the two tabs and ⌃` for the terminal panel.
///
/// The tab bindings are absolute rather than a toggle, so the chord you press
/// doesn't depend on the tab you're already on — GitHub Desktop's pair, and
/// the Tauri client binds the same two beside its own ⌘L toggle.
struct ViewCommands: Commands {
    @FocusedValue(\.tabCommand) private var tabCommand: TabCommand?
    @FocusedValue(\.terminalCommand) private var terminalCommand: TerminalCommand?

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Button("Changes") { tabCommand?.select(.changes) }
                .keyboardShortcut("1", modifiers: .command)
                .disabled(tabCommand == nil)

            Button("History") { tabCommand?.select(.history) }
                .keyboardShortcut("2", modifiers: .command)
                .disabled(tabCommand == nil)

            Divider()

            // ⌃` and not ⌘`, which macOS owns for cycling an app's windows.
            // The chord lives here rather than on the panel's own toggle so
            // it still fires while the emulator holds focus — the one place
            // it is most needed, and the one place a key equivalent on a
            // button would never be seen.
            Button(terminalCommand?.isExpanded == true ? "Hide Terminal" : "Show Terminal") {
                terminalCommand?.toggle()
            }
            .keyboardShortcut("`", modifiers: .control)
            .disabled(terminalCommand == nil)

            Divider()

            // The View-menu home of the reload the toolbar Refresh button
            // used to carry — the toolbar's one button is the adaptive sync
            // control now. Posted as a notification because it needs neither
            // a state-dependent title nor enablement: the listener ignores it
            // while no repo is open or a transfer is running.
            Button("Refresh") {
                NotificationCenter.default.post(name: .leogitRefreshRequested, object: nil)
            }
            .keyboardShortcut("r")
        }
    }
}

// MARK: - Branch

/// The toolbar branch control's items, a second time, in the menu bar.
///
/// Both surfaces render one `BranchMenuContent`, so the two cannot drift into
/// offering different branch actions — the failure mode this whole plan keeps
/// finding. What differs is only where each item's closure lands: the toolbar
/// menu flips its own state directly, while these post, because the sheets and
/// confirmations an action opens belong to the toolbar control.
struct BranchCommands: Commands {
    @FocusedValue(\.branchCommand) private var branchCommand: BranchCommand?

    var body: some Commands {
        CommandMenu("Branch") {
            if let branchCommand {
                BranchMenuContent(command: branchCommand, bindsShortcuts: true)
            } else {
                // A menu that vanishes with the repository would move every
                // other menu along the bar; one disabled item holds its place
                // and says why it is empty.
                Button("No Repository Open") {}
                    .disabled(true)
            }
        }
    }
}

// MARK: - Focused values

/// How to change which tab the repository screen is showing. Its presence is
/// also the answer to "is there a repository open" — the items disable on a
/// `nil` value rather than carrying a flag that could disagree with it.
struct TabCommand {
    var select: (RepoTab) -> Void
}

/// Whether the terminal panel is open, and how to toggle it.
struct TerminalCommand {
    var isExpanded: Bool
    var toggle: () -> Void
}

extension FocusedValues {
    @Entry var tabCommand: TabCommand?
    @Entry var terminalCommand: TerminalCommand?
}

import SwiftUI

@main
struct LeoGitApp: App {
    @State private var store = RepoStore()

    init() {
        // The process's first Rust call, before any other thread could be
        // reading the environment: repair the minimal PATH a Finder launch
        // inherits so `git`, `gh`, and the `claude` CLI resolve.
        GitBridge.bootstrapPathEnvironment()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(store)
                .task { await store.loadCoreVersion() }
        }
        .defaultSize(width: 980, height: 660)
        .windowToolbarStyle(.unified)
        .commands {
            CommandGroup(replacing: .newItem) {}

            // The View-menu home of the reload the toolbar Refresh button
            // used to carry — the toolbar's one button is the adaptive sync
            // control now. Posted as a notification because commands live on
            // the scene while the stores live in ContentView; the listener
            // ignores it while no repo is open or a transfer is running.
            CommandGroup(after: .toolbar) {
                Button("Refresh") {
                    NotificationCenter.default.post(name: .leogitRefreshRequested, object: nil)
                }
                .keyboardShortcut("r")
            }

            RepositoryCommands()
        }

        // Gives the app the standard "LeoGit ▸ Settings…" menu item and ⌘,
        // for free; the window edits the same config.toml the Tauri client
        // reads, so a change here applies to both.
        Settings {
            SettingsView()
        }
    }
}

/// The menu-bar home of the toolbar's adaptive sync button: one item that
/// renames itself to whatever the button proposes — Publish, Publish Branch,
/// Pull, Push, Fetch — and runs the same closure under ⌘P.
///
/// The action arrives as a focused scene value — unlike ⌘R's plain
/// notification — because this item's *title* depends on repository state:
/// a notification could fire the action but not label it, and a menu item
/// that lies about what it does is worse than no shortcut. Its perform
/// closure still hops through a notification, since the sheet and alert
/// the action may open live with the toolbar button.
private struct RepositoryCommands: Commands {
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

extension Notification.Name {
    /// Posted by the View ▸ Refresh command (⌘R); the main window's content
    /// view performs the actual reload, since the stores live there.
    static let leogitRefreshRequested = Notification.Name("leogitRefreshRequested")

    /// Posted by Repository ▸ <sync action> (⌘P); `SyncControls` performs
    /// the proposed action, so its sheet, alert, and busy handling stay on
    /// the same path a button click takes.
    static let leogitSyncActionRequested = Notification.Name("leogitSyncActionRequested")
}

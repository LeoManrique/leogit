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
        }

        // Gives the app the standard "LeoGit ▸ Settings…" menu item and ⌘,
        // for free; the window edits the same config.toml the Tauri client
        // reads, so a change here applies to both.
        Settings {
            SettingsView()
        }
    }
}

extension Notification.Name {
    /// Posted by the View ▸ Refresh command (⌘R); the main window's content
    /// view performs the actual reload, since the stores live there.
    static let leogitRefreshRequested = Notification.Name("leogitRefreshRequested")
}

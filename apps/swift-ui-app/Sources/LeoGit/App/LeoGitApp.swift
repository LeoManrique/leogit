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
        }

        // Gives the app the standard "LeoGit ▸ Settings…" menu item and ⌘,
        // for free; the window edits the same config.toml the Tauri client
        // reads, so a change here applies to both.
        Settings {
            SettingsView()
        }
    }
}

import SwiftUI

@main
struct LeoGitApp: App {
    /// Only AppKit is told when a folder is opened from outside the app, so
    /// the delegate exists to catch `application(_:open:)` — and owns the
    /// store it publishes into, which the scene hands down to the root view.
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    @State private var store = RepoStore()

    /// The one native owner of the shared config (see its doc comment for
    /// the reload sites). Created here so the main window and the Settings
    /// scene observe the same instance — the pair the Tauri client gets from
    /// its single `$config` store.
    @State private var appConfig = AppConfigStore()

    init() {
        // The process's first Rust call, before any other thread could be
        // reading the environment: repair the minimal PATH a Finder launch
        // inherits so `git`, `gh`, and the `claude` CLI resolve.
        GitBridge.bootstrapPathEnvironment()
    }

    var body: some Scene {
        WindowGroup {
            ContentView(appConfig: appConfig)
                .environment(store)
                .environment(appConfig)
                .environment(appDelegate.launch)
                .task {
                    await store.loadCoreVersion()
                    await appConfig.reload()
                }
        }
        .defaultSize(width: 980, height: 660)
        .windowToolbarStyle(.unified)
        // The menu bar is the app's own documentation for the chords it
        // answers; see `AppMenus.swift` for why each item is shaped the way
        // it is.
        .commands {
            FileCommands()
            ViewCommands()
            BranchCommands()
            RepositoryCommands()
        }

        // Gives the app the standard "LeoGit ▸ Settings…" menu item and ⌘,
        // for free; the window edits the same config.toml the Tauri client
        // reads, so a change here applies to both.
        Settings {
            SettingsView(appConfig: appConfig)
                .environment(appConfig)
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

    /// Posted when a Settings save changes `scan_paths` or `scan_depth` — what
    /// discovery walks. The main window re-walks; the Settings scene is a
    /// separate scene and cannot reach `RepoDirectoryStore` directly.
    static let leogitScanPathsChanged = Notification.Name("leogitScanPathsChanged")

    /// Posted by File ▸ Clone Repository… (⇧⌘O). The root view presents the
    /// sheet, so the item works with or without a repository open.
    static let leogitCloneRequested = Notification.Name("leogitCloneRequested")

    /// Posted by a Branch menu item; the object is the `BranchAction` it
    /// stands for. `BranchMenu` performs it, so the sheets and confirmations
    /// the action opens stay with the toolbar control a click would have used.
    static let leogitBranchActionRequested = Notification.Name("leogitBranchActionRequested")
}

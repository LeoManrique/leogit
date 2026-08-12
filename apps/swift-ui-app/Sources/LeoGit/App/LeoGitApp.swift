import SwiftUI

@main
struct LeoGitApp: App {
    @State private var store = RepoStore()

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
    }
}

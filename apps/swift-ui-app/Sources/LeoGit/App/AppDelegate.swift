import AppKit

/// The app's `NSApplicationDelegate`, which exists for exactly one reason:
/// AppKit delivers folders opened from outside the app through a delegate
/// callback, and SwiftUI has no scene-level equivalent for it.
///
/// `Info.plist` declares `public.folder` under `CFBundleDocumentTypes`, which
/// is what makes LaunchServices route a folder here — and, for free, what puts
/// LeoGit in Finder's *Open With* menu and lets a folder be dropped on the
/// Dock icon. The rank is `Alternate`: the app can open a folder when asked,
/// but never competes with Finder to become the machine's default handler for
/// every folder on it.
///
/// The delegate owns the [`LaunchStore`] rather than reaching for a singleton
/// so the scene can hand the same instance to its views — `@Observable` types
/// are not injected into the environment automatically the way an
/// `ObservableObject` delegate would be.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let launch = LaunchStore()

    /// A folder arrived from LaunchServices: `open -a LeoGit <dir>`, a drop on
    /// the Dock icon, or Finder's Open With.
    /// Cold start delivers this between `applicationWillFinishLaunching`
    /// and `applicationDidFinishLaunching` — before any SwiftUI task runs — so
    /// the launch path finds it already queued; a warm one arrives whenever
    /// the user asks, and the root view switches repositories.
    func application(_ application: NSApplication, open urls: [URL]) {
        launch.open(urls: urls)
    }
}
